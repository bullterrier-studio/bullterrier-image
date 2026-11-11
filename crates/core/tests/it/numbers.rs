//! The numbers of a recipe by address (`Recipe::numbers`, `number`, `with_value`): what a sweep varies. Expected values
//! are worked out by hand from the recipe and the ranges the effects, regions and frame document.

use bullterrier_image::region::{Band, Bands, LinearGradient, RadialGradient, Shards, Subject, Svg};
use bullterrier_image::{Drive, Each, Entry, Error, Frame, Presets, Recipe, Region, Step};
use serde_json::{Value, json};

use crate::common::Outcome;

fn addresses(recipe: &Recipe) -> Result<Vec<String>, Error> {
    Ok(recipe.numbers()?.into_iter().map(|number| number.address).collect())
}

fn json(recipe: &Recipe) -> Result<Value, serde_json::Error> {
    serde_json::to_value(recipe)
}

fn refused(result: Result<Recipe, Error>) -> String {
    result.err().map(|error| error.to_string()).unwrap_or_default()
}

fn pulse() -> Recipe {
    Recipe::new([Step::new("grade").param("exposure", 0.3)])
}

#[test]
fn a_step_has_the_numbers_its_effect_declares_then_its_opacity() -> Outcome {
    let found = addresses(&pulse())?;
    assert_eq!(found.first().map(String::as_str), Some("grade.temperature"));
    let at = |address: &str| found.iter().position(|found| found == address);
    assert_eq!(
        at("grade.opacity").map(|opacity| opacity + 1),
        at("frame.angle"),
        "opacity ends the step's numbers"
    );
    assert!(found.contains(&"grade.contrast".to_owned()), "{found:?}");
    assert!(
        found.contains(&"frame.angle".to_owned()),
        "the frame's numbers end the list: {found:?}"
    );
    let exposure = pulse().number("grade.exposure")?;
    assert!(
        !exposure.whole && !exposure.cyclic && exposure.min < 0.0 && exposure.max > 0.0,
        "{exposure:?}"
    );
    Ok(())
}

#[test]
fn a_step_is_addressed_by_its_name_its_effect_or_the_effect_and_its_number() -> Outcome {
    let two = Recipe::new([
        Step::new("grade").param("exposure", 0.1),
        Step::new("grade").param("exposure", 0.2),
        Step::new("pixel_grid"),
    ]);
    let found = addresses(&two)?;
    assert!(found.contains(&"grade#1.exposure".to_owned()) && found.contains(&"grade#2.exposure".to_owned()));
    assert!(
        found.iter().any(|address| address.starts_with("pixel_grid.")),
        "{found:?}"
    );
    let changed = two.with_value("grade#2.exposure", 0.9, &Presets::new())?;
    assert_eq!(json(&changed)?["steps"][1]["params"]["exposure"], json!(0.9));
    assert_eq!(
        json(&changed)?["steps"][0]["params"]["exposure"],
        json!(0.1),
        "the other step is left"
    );
    let ambiguous = refused(two.with_value("grade.exposure", 0.5, &Presets::new()));
    assert!(
        ambiguous.contains("2 \"grade\" steps") && ambiguous.contains("grade#1.exposure, grade#2.exposure"),
        "{ambiguous}"
    );
    let beyond = refused(two.with_value("grade#3.exposure", 0.5, &Presets::new()));
    assert!(beyond.contains("no step \"grade#3\""), "{beyond}");
    let (mut first, mut second) = (
        Step::new("grade").param("exposure", 0.1),
        Step::new("grade").param("exposure", 0.2),
    );
    (first.name, second.name) = (Some("one".to_owned()), Some("two".to_owned()));
    let both = Recipe::new([first, second]);
    let second_one = both.with_value("grade#2.exposure", 0.8, &Presets::new())?;
    assert_eq!(
        json(&second_one)?["steps"][1]["params"]["exposure"],
        json!(0.8),
        "by effect and number, though named"
    );
    assert_eq!(json(&second_one)?["steps"][0]["params"]["exposure"], json!(0.1));
    let mut named = Step::new("grade").param("exposure", 0.1);
    named.name = Some("pulse".to_owned());
    let by_name = Recipe::new([named]);
    assert!(addresses(&by_name)?.contains(&"pulse.exposure".to_owned()));
    assert_eq!(
        by_name.number("grade.exposure")?.address,
        "pulse.exposure",
        "the only step of its effect, though named"
    );
    Ok(())
}

#[test]
fn a_value_is_set_in_a_copy_and_a_whole_number_takes_only_a_whole_value() -> Outcome {
    let original = pulse();
    let changed = original.with_value("grade.exposure", -1.5, &Presets::new())?;
    assert_eq!(json(&changed)?["steps"][0]["params"]["exposure"], json!(-1.5));
    assert_eq!(
        json(&original)?["steps"][0]["params"]["exposure"],
        json!(0.3),
        "the original is left as it is"
    );
    let whole = Recipe::new([Step::new("slice_shift")]);
    let bands = whole.number("slice_shift.bands")?;
    assert!(bands.whole, "{bands:?}");
    assert_eq!(
        json(&whole.with_value("slice_shift.bands", 6.0, &Presets::new())?)?["steps"][0]["params"]["bands"],
        json!(6)
    );
    let fractional = refused(whole.with_value("slice_shift.bands", 6.5, &Presets::new()));
    assert!(
        fractional.contains("whole number") && fractional.contains("slice_shift.bands"),
        "{fractional}"
    );
    Ok(())
}

#[test]
fn a_value_out_of_its_range_or_not_a_number_is_refused_naming_the_address() {
    for (value, says) in [(99.0, "exposure"), (f64::NAN, "finite"), (f64::INFINITY, "finite")] {
        let said = refused(pulse().with_value("grade.exposure", value, &Presets::new()));
        assert!(
            said.contains("\"grade.exposure\"") && said.contains(says),
            "{value}: {said}"
        );
    }
    let opacity = refused(pulse().with_value("grade.opacity", 1.5, &Presets::new()));
    assert!(
        opacity.contains("\"grade.opacity\"") && opacity.contains("opacity"),
        "{opacity}"
    );
}

#[test]
fn a_drive_is_set_through_its_start_end_and_levels() -> Outcome {
    let drive = Recipe::new([Step::new("grade").param("exposure", Drive::new("depth", 0.0, 1.0))]);
    let found = addresses(&drive)?;
    for part in ["start", "end", "levels"] {
        assert!(found.contains(&format!("grade.exposure.{part}")), "{found:?}");
    }
    assert!(!found.contains(&"grade.exposure".to_owned()));
    let end = drive.with_value("grade.exposure.end", 2.0, &Presets::new())?;
    assert_eq!(json(&end)?["steps"][0]["params"]["exposure"]["end"], json!(2.0));
    let levels = drive.with_value("grade.exposure.levels", 8.0, &Presets::new())?;
    assert_eq!(json(&levels)?["steps"][0]["params"]["exposure"]["levels"], json!(8));
    assert!(refused(drive.with_value("grade.exposure.levels", 40.0, &Presets::new())).contains("levels"));
    let alone = refused(drive.with_value("grade.exposure", 1.0, &Presets::new()));
    assert!(
        alone.contains("is driven by a field: set exposure.start, exposure.end or exposure.levels"),
        "{alone}"
    );
    let undriven = refused(pulse().with_value("grade.exposure.start", 1.0, &Presets::new()));
    assert!(
        undriven.contains("is not driven by a field, so it has no \"start\""),
        "{undriven}"
    );
    Ok(())
}

#[test]
fn an_each_step_has_its_feather_opacity_seed_and_the_numbers_of_its_looks() -> Outcome {
    let recipe = Recipe::new([Entry::Each(Each::new(
        "panes",
        [Some(Step::new("grade").param("exposure", 0.1)), None],
    ))])
    .with_region("panes", bullterrier_image::region::Grid::default());
    let found = addresses(&recipe)?;
    for address in [
        "each.feather",
        "each.opacity",
        "each.seed",
        "each.looks[0].exposure",
        "each.looks[0].opacity",
    ] {
        assert!(found.contains(&address.to_owned()), "{address}: {found:?}");
    }
    assert!(
        found.iter().all(|address| !address.contains("looks[1]")),
        "a look that is none has no numbers"
    );
    let changed = recipe.with_value("each.looks[0].exposure", 0.7, &Presets::new())?;
    assert_eq!(
        json(&changed)?["steps"][0]["looks"][0]["params"]["exposure"],
        json!(0.7)
    );
    let feather = recipe.with_value("each.feather", 3.0, &Presets::new())?;
    assert_eq!(json(&feather)?["steps"][0]["feather"], json!(3.0));
    assert!(recipe.number("each.seed")?.whole);
    Ok(())
}

#[test]
fn the_numbers_of_a_region_and_of_the_frame_are_addressed_by_their_names() -> Outcome {
    let recipe = pulse().with_region(
        "near",
        Band {
            of: "subject".to_owned(),
            low: 0.2,
            high: 0.6,
            ..Band::default()
        },
    );
    assert_eq!(recipe.number("region.near.low")?.address, "region.near.low");
    let moved = recipe.with_value("region.near.low", 0.4, &Presets::new())?;
    assert_eq!(json(&moved)?["regions"]["near"]["low"], json!(0.4));
    let turned = recipe.with_value("frame.angle", 400.0, &Presets::new())?;
    assert_eq!(
        json(&turned)?["frame"]["angle"],
        json!(400.0),
        "a cyclic angle takes any value"
    );
    assert!(recipe.number("frame.angle")?.cyclic);
    let above = refused(recipe.with_value("region.near.low", 0.9, &Presets::new()));
    assert!(
        above.contains("\"region.near.low\"") && above.contains("`low`"),
        "{above}"
    );
    let tall = refused(recipe.with_value("frame.margin", 3.0, &Presets::new()));
    assert!(tall.contains("\"frame.margin\"") && tall.contains("margin"), "{tall}");
    Ok(())
}

#[test]
fn an_address_that_points_at_nothing_names_what_there_is_and_the_nearest() {
    let recipe = pulse().with_region(
        "near",
        Band {
            of: "subject".to_owned(),
            ..Band::default()
        },
    );
    let cases = [
        ("exposure", "not an address"),
        ("grade.exposur", "did you mean grade.exposure"),
        ("grade.nothing", "its numbers: temperature, tint, exposure"),
        ("gradee.exposure", "it has: grade, region.near, frame"),
        ("region.far.low", "the recipe's regions: near"),
        ("region.near", "addressed `region.near.number`"),
        ("frame.nothing", "'frame' has no number \"nothing\""),
    ];
    for (address, says) in cases {
        let said = refused(recipe.with_value(address, 0.5, &Presets::new()));
        assert!(
            said.starts_with(&format!("{address:?}: ")) || said.contains(&format!("{address:?}")),
            "{address}: {said}"
        );
        assert!(said.contains(says), "{address}: expected {says:?} in {said}");
    }
}

/// The top-level numbers of the region of this kind with default values.
fn scalars(region: &Region) -> Result<Vec<String>, serde_json::Error> {
    let Value::Object(fields) = serde_json::to_value(region)? else {
        return Ok(Vec::new());
    };
    Ok(fields
        .into_iter()
        .filter(|(_, value)| value.is_number())
        .map(|(field, _)| field)
        .collect())
}

#[test]
fn every_number_of_a_region_kind_or_the_frame_has_a_row_unless_it_is_listed_as_not_a_number_to_sweep() -> Outcome {
    let kinds: [Region; 6] = [
        Shards::default().into(),
        Svg::default().into(),
        Subject::default().into(),
        Band::default().into(),
        Bands::default().into(),
        LinearGradient::default().into(),
    ];
    let all: [Region; 7] = [
        kinds[0].clone(),
        kinds[1].clone(),
        kinds[2].clone(),
        kinds[3].clone(),
        kinds[4].clone(),
        kinds[5].clone(),
        RadialGradient::default().into(),
    ];
    for region in all {
        let recipe = pulse().with_region("r", region.clone());
        let have = addresses(&recipe)?;
        for field in scalars(&region)? {
            assert!(
                have.contains(&format!("region.r.{field}")),
                "{region:?}: `{field}` is a number with no row in `REGION_ROWS`"
            );
        }
    }
    let frame = serde_json::to_value(Frame::default())?;
    let have = addresses(&pulse())?;
    for (field, value) in frame.as_object().into_iter().flatten() {
        if value.is_number() {
            assert!(
                have.contains(&format!("frame.{field}")),
                "`frame.{field}` has no row in `FRAME_ROWS`"
            );
        }
    }
    Ok(())
}

#[test]
fn a_rows_range_is_the_range_the_check_holds_the_number_to() -> Outcome {
    let recipe = pulse()
        .with_region("s", Shards::default())
        .with_region("v", Svg::default())
        .with_region("c", Subject::default())
        .with_region(
            "b",
            Band {
                of: "subject".to_owned(),
                ..Band::default()
            },
        )
        .with_region(
            "n",
            Bands {
                of: "subject".to_owned(),
                ..Bands::default()
            },
        )
        .with_region("g", LinearGradient::default())
        .with_region("d", RadialGradient::default());
    let presets = Presets::new();
    for number in recipe.numbers()? {
        if !(number.address.starts_with("region.") || number.address.starts_with("frame.")) || number.cyclic {
            continue;
        }
        let (low, high) = (number.min, number.max);
        for edge in [low, high] {
            let said = recipe.with_value(&number.address, edge, &presets);
            // `band.low` and `band.high` bound each other: the edge of one is refused by the other's current value
            let ordered = matches!(number.address.rsplit('.').next(), Some("low" | "high"));
            assert!(
                said.is_ok() || ordered,
                "{} at its edge {edge}: {said:?}",
                number.address
            );
        }
        let step = if number.whole { 1.0 } else { 0.001 };
        for beyond in [low - step, high + step] {
            let said = refused(recipe.with_value(&number.address, beyond, &presets));
            assert!(
                said.contains(&format!("{:?}", number.address)),
                "{} at {beyond} is accepted",
                number.address
            );
        }
    }
    Ok(())
}

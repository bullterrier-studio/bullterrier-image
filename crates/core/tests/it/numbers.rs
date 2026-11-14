//! The numbers of a recipe by address (`Recipe::numbers`, `number`, `with_value`, `with_values`): what a sweep varies. Expected values
//! are worked out by hand from the recipe and the ranges the effects, regions and frame document.

use bullterrier_image::region::{
    Band, Bands, ColorRange, Components, LinearGradient, RadialGradient, Shards, Subject, Svg,
};
use bullterrier_image::{
    Drive, Each, Entry, Error, Frame, Home, Layer, Options, Photo, Picture, Presets, Recipe, Region, Step,
};
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
        ambiguous.contains("2 'grade' steps") && ambiguous.contains("grade#1.exposure, grade#2.exposure"),
        "{ambiguous}"
    );
    let beyond = refused(two.with_value("grade#3.exposure", 0.5, &Presets::new()));
    assert!(beyond.contains("no step 'grade#3'"), "{beyond}");
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
            said.contains("'grade.exposure'") && said.contains(says),
            "{value}: {said}"
        );
    }
    let opacity = refused(pulse().with_value("grade.opacity", 1.5, &Presets::new()));
    assert!(
        opacity.contains("'grade.opacity'") && opacity.contains("opacity"),
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
        undriven.contains("is not driven by a field, so it has no 'start'"),
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
        above.contains("'region.near.low'") && above.contains("`low`"),
        "{above}"
    );
    let tall = refused(recipe.with_value("frame.margin", 3.0, &Presets::new()));
    assert!(tall.contains("'frame.margin'") && tall.contains("margin"), "{tall}");
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
        ("frame.nothing", "'frame' has no number 'nothing'"),
    ];
    for (address, says) in cases {
        let said = refused(recipe.with_value(address, 0.5, &Presets::new()));
        assert!(said.starts_with(&format!("'{address}': ")), "{address}: {said}");
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
    let all: [Region; 9] = [
        Shards::default().into(),
        Svg::default().into(),
        Subject::default().into(),
        Band::default().into(),
        Bands::default().into(),
        Components::default().into(),
        ColorRange::default().into(),
        LinearGradient::default().into(),
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
        .with_region(
            "w",
            Components {
                of: "subject".to_owned(),
                ..Components::default()
            },
        )
        .with_region("k", ColorRange::default())
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
                said.contains(&format!("'{}'", number.address)),
                "{} at {beyond} is accepted",
                number.address
            );
        }
    }
    Ok(())
}

#[test]
fn a_number_says_its_value_now_its_unit_and_what_it_does() -> Outcome {
    let recipe = Recipe::new([
        Step::new("grade").param("exposure", 0.3),
        Step::new("ambient_occlusion"),
        Step::new("gaussian_blur").param("radius", Drive::new("depth", 2.0, 8.0)),
    ])
    .with_region(
        "near",
        Band {
            of: "subject".to_owned(),
            low: 0.2,
            high: 0.6,
            ..Band::default()
        },
    );
    let value = |address: &str| recipe.number(address).map(|number| number.value);
    assert_eq!(value("grade.exposure")?, Some(0.3), "written");
    assert_eq!(
        value("grade.contrast")?,
        Some(1.0),
        "the default where it is not written"
    );
    assert_eq!(
        value("ambient_occlusion.opacity")?,
        Some(0.6),
        "the effect's own opacity where the step sets none"
    );
    assert_eq!(value("gaussian_blur.radius.start")?, Some(2.0));
    assert_eq!(value("gaussian_blur.radius.end")?, Some(8.0));
    assert_eq!(value("gaussian_blur.radius.levels")?, Some(5.0));
    assert_eq!(value("region.near.low")?, Some(0.2));
    assert_eq!(value("frame.angle")?, Some(0.0));
    assert_eq!(
        value("frame.aspect")?,
        None,
        "an optional number the recipe leaves unset"
    );
    let exposure = recipe.number("grade.exposure")?;
    assert_eq!(exposure.unit, "stops");
    assert!(
        exposure.doc.starts_with("Brightness in photographic stops"),
        "{}",
        exposure.doc
    );
    assert_eq!(recipe.number("region.near.low")?.unit, "fraction");
    assert_eq!(recipe.number("frame.angle")?.unit, "degrees");
    assert!(recipe.number("frame.angle")?.doc.len() > 20);
    assert_eq!(recipe.number("gaussian_blur.radius.levels")?.unit, "count");
    Ok(())
}

#[test]
fn an_address_is_located_in_a_step_a_region_or_the_frame_with_the_rest_of_it() -> Result<(), Error> {
    let recipe = Recipe::new([Step::new("grade"), Step::new("pixel_grid")]).with_region(
        "near",
        Band {
            of: "subject".to_owned(),
            ..Band::default()
        },
    );
    let found = recipe.locate("pixel_grid.opacity")?;
    assert_eq!((found.place, found.rest.as_str()), (Home::Step(1), "opacity"));
    let found = recipe.locate("region.near.high")?;
    assert_eq!(
        (found.place, found.rest.as_str()),
        (Home::Region("near".to_owned()), "high")
    );
    let found = recipe.locate("frame.margin")?;
    assert_eq!((found.place, found.rest.as_str()), (Home::Frame, "margin"));
    assert_eq!(found.number.address, "frame.margin");
    Ok(())
}

// --- one convention for directions and lengths -------------------------------------------------------------------
// A number is a direction (cyclic degrees, 0 to 360, clockwise from the right), a rotation (cyclic, -180 to 180,
// clockwise when positive), a length (px) or a share of something named. The lists below hold each number of a region
// or the frame that breaks the convention by its name, with the reason.

/// Degrees numbers that are how far something is turned although named `angle`: the reason each is a rotation.
const ROTATIONS: &[(&str, &str)] = &[("frame.angle", "how far the photo is turned before the steps")];
/// Lengths written as a share, with what the share is of.
const LENGTHS_AS_SHARES: &[(&str, &str)] = &[
    ("region.radial_gradient.radius", "the picture's shorter side"),
    ("region.band.feather", "the field's range of values"),
    (
        "region.color_range.feather",
        "the Oklab distance from black to white, beyond the tolerance",
    ),
    ("frame.margin", "the longer side of the subject's box"),
    ("frame.corner_radius", "the picture's shorter side"),
];
const ROTATION_WORDS: &[&str] = &["turn", "rotate", "rotation"];
const DIRECTION_WORDS: &[&str] = &["angle", "direction"];
const LENGTH_WORDS: &[&str] = &[
    "length",
    "width",
    "size",
    "offset",
    "height",
    "spacing",
    "radius",
    "span",
    "gap",
    "cell",
    "thickness",
    "distance",
    "reach",
    "margin",
    "feather",
    "wavelength",
    "amplitude",
];
const POSITIONS: &[&str] = &["x", "y", "at", "base"]; // a position is a fraction of each axis, not a length

/// Whether `doc` says it runs clockwise (not "counter-clockwise" or "anticlockwise").
fn says_clockwise(doc: &str) -> bool {
    let doc = doc.to_lowercase();
    doc.match_indices("clockwise").any(|(at, _)| {
        !doc[..at]
            .chars()
            .next_back()
            .is_some_and(|before| before.is_ascii_lowercase() || before == '-')
    })
}

/// What breaks the convention among `numbers`, as one line each.
fn convention_problems(numbers: &[Item]) -> Vec<String> {
    let mut problems = Vec::new();
    for number in numbers {
        let address = number.address.as_str();
        let field = address.rsplit('.').next().unwrap_or(address);
        let words: Vec<&str> = field.split('_').collect();
        if number.unit == "degrees" {
            let rotation = words.iter().any(|word| ROTATION_WORDS.contains(word))
                || ROTATIONS.iter().any(|(listed, _)| *listed == address);
            let (low, high, what) = if rotation {
                (-180.0, 180.0, "rotation")
            } else if words.iter().any(|word| DIRECTION_WORDS.contains(word)) {
                (0.0, 360.0, "direction")
            } else {
                problems.push(format!("{address} is in degrees but no direction or rotation"));
                continue;
            };
            if !number.cyclic || (number.min, number.max) != (low, high) {
                problems.push(format!(
                    "{address} is a {what}: cyclic, {low} to {high}, not {} to {}",
                    number.min, number.max
                ));
            }
            if !says_clockwise(&number.doc) {
                problems.push(format!("{address} is a {what}: its doc must say it runs clockwise"));
            }
        }
        let named: Vec<&str> = words
            .iter()
            .copied()
            .filter(|word| !["x", "y", "min", "max"].contains(word))
            .collect();
        let last = named.last().copied().unwrap_or(field);
        let length = LENGTH_WORDS.contains(&last) && !POSITIONS.contains(&field);
        if number.unit == "fraction"
            && (length || number.doc.contains("long side"))
            && !LENGTHS_AS_SHARES.iter().any(|(listed, _)| *listed == address)
        {
            problems.push(format!(
                "{address} is a length in a share: give it in px, or list it in LENGTHS_AS_SHARES"
            ));
        }
    }
    problems
}

/// A number as the convention reads it.
struct Item {
    address: String,
    unit: String,
    min: f64,
    max: f64,
    cyclic: bool,
    doc: String,
}

fn named_number(address: &str, unit: &str, min: f64, max: f64, cyclic: bool, doc: &str) -> Item {
    Item {
        address: address.to_owned(),
        unit: unit.to_owned(),
        min,
        max,
        cyclic,
        doc: doc.to_owned(),
    }
}

#[test]
fn the_convention_check_flags_what_breaks_it_and_passes_what_keeps_it() {
    let good = [
        named_number(
            "region.r.angle",
            "degrees",
            0.0,
            360.0,
            true,
            "Clockwise from pointing right.",
        ),
        named_number(
            "region.r.turn",
            "degrees",
            -180.0,
            180.0,
            true,
            "Clockwise when positive.",
        ),
        named_number("region.r.width", "px", 0.0, 100.0, false, "How wide."),
        named_number("region.r.x", "fraction", 0.0, 1.0, false, "Where across."),
    ];
    assert_eq!(convention_problems(&good), Vec::<String>::new());
    let bad = [
        (
            "a direction over -180..180",
            named_number("region.r.angle", "degrees", -180.0, 180.0, true, "Clockwise."),
        ),
        (
            "a direction that is not cyclic",
            named_number("region.r.angle", "degrees", 0.0, 360.0, false, "Clockwise."),
        ),
        (
            "a direction that runs the other way",
            named_number("region.r.angle", "degrees", 0.0, 360.0, true, "Counter-clockwise."),
        ),
        (
            "a rotation over 0..360",
            named_number("region.r.turn", "degrees", 0.0, 360.0, true, "Clockwise."),
        ),
        (
            "an unlisted degrees field",
            named_number("region.r.lean", "degrees", 0.0, 45.0, false, "How far it leans."),
        ),
        (
            "a length in a share",
            named_number("region.r.width", "fraction", 0.0, 1.0, false, "How wide."),
        ),
    ];
    for (what, number) in bad {
        assert_eq!(convention_problems(&[number]).len(), 1, "{what}");
    }
}

#[test]
fn every_direction_rotation_and_length_of_a_region_or_the_frame_keeps_the_convention() -> Outcome {
    let regions: [(&str, Region); 9] = [
        ("shards", Shards::default().into()),
        ("svg", Svg::default().into()),
        ("subject", Subject::default().into()),
        ("band", Band::default().into()),
        ("bands", Bands::default().into()),
        ("components", Components::default().into()),
        ("color_range", ColorRange::default().into()),
        ("linear_gradient", LinearGradient::default().into()),
        ("radial_gradient", RadialGradient::default().into()),
    ];
    let mut recipe = pulse();
    for (name, region) in regions {
        recipe = recipe.with_region(name, region);
    }
    let numbers: Vec<Item> = recipe
        .numbers()?
        .into_iter()
        .filter(|number| number.address.starts_with("region.") || number.address.starts_with("frame."))
        .map(|number| Item {
            address: number.address,
            unit: number.unit,
            min: number.min,
            max: number.max,
            cyclic: number.cyclic,
            doc: number.doc,
        })
        .collect();
    assert_eq!(convention_problems(&numbers), Vec::<String>::new());
    // the lists name only numbers that exist and are still what they are listed as
    let have: Vec<&str> = numbers.iter().map(|number| number.address.as_str()).collect();
    for (listed, _) in ROTATIONS.iter().chain(LENGTHS_AS_SHARES) {
        assert!(have.contains(listed), "{listed} is listed but is no number");
    }
    Ok(())
}

// --- `with_values`: several numbers at once, all or nothing -----------------------------------------------------------

/// Recipes that between them have every kind of address: a plain step, a named step, a driven number, an each step with
/// looks, a region and the frame, and two steps of one effect.
fn recipes() -> Result<Vec<(&'static str, Recipe)>, Error> {
    let from = |text: Value| Recipe::from_json(&text.to_string());
    Ok(vec![
        ("plain", pulse()),
        (
            "named",
            from(json!({"steps": [{"effect": "grade", "name": "pulse", "params": {"exposure": 0.3}}]}))?,
        ),
        (
            "driven",
            from(
                json!({"steps": [{"effect": "grade", "params": {"exposure": {"by": "depth", "start": 0.0, "end": 1.0}}}]}),
            )?,
        ),
        (
            "each",
            from(json!({
                "steps": [{"each": "panes", "looks": [{"effect": "grade", "params": {"exposure": 0.1}}, null], "feather": 0.5}],
                "regions": {"panes": {"kind": "grid"}},
            }))?,
        ),
        (
            "region and frame",
            from(json!({
                "steps": [{"effect": "grade", "mask": "near"}],
                "regions": {"near": {"kind": "band", "of": "subject", "low": 0.2, "high": 0.6}},
                "frame": {"angle": 30.0},
            }))?,
        ),
        (
            "two of one effect",
            from(
                json!({"steps": [{"effect": "grade", "params": {"exposure": 0.1}}, {"effect": "grade", "params": {"exposure": 0.2}}]}),
            )?,
        ),
    ])
}

/// What `recipe` renders on a small picture, as bits, so that a difference of one bit is seen.
fn bits(recipe: &Recipe) -> Result<Vec<u32>, Error> {
    let values: Vec<f32> = (0..4 * 6 * 3_u16).map(|at| f32::from(at % 7) / 7.0).collect();
    let ramp: Vec<f32> = (0..24_u16).map(|at| f32::from(at) / 24.0).collect();
    let photo = Photo::new(Picture::new(4, 6, &values)?)
        .with_map("subject", Layer::new(4, 6, ramp.clone())?)?
        .with_map("depth", Layer::new(4, 6, ramp)?)?;
    let out = photo.render(recipe, &Presets::new(), Options::default())?;
    Ok(out.into_values().into_iter().map(f32::to_bits).collect())
}

#[test]
fn setting_every_number_to_its_own_value_gives_the_same_recipe() -> Outcome {
    for (kind, recipe) in recipes()? {
        let before = recipe.numbers()?;
        let own: Vec<(&str, f64)> = before
            .iter()
            .filter_map(|number| Some((number.address.as_str(), number.value?)))
            .collect();
        assert!(own.len() > 3, "{kind}: {own:?}");
        let same = recipe.with_values(&own, &Presets::new())?;
        assert_eq!(same.numbers()?, before, "{kind}");
        assert_eq!(bits(&same)?, bits(&recipe)?, "{kind} renders differently");
        for (address, value) in own {
            let one = recipe.with_value(address, value, &Presets::new())?;
            assert_eq!(one.numbers()?, before, "{kind}: {address}");
        }
    }
    Ok(())
}

#[test]
fn a_new_value_changes_exactly_that_number() -> Outcome {
    let rows = [
        ("plain", "grade.exposure", 0.9),
        ("plain", "grade.opacity", 0.5),
        ("named", "pulse.exposure", -0.4),
        ("named", "grade.exposure", -0.4), // the effect reaches a named step that is the only one
        ("driven", "grade.exposure.start", 0.25),
        ("driven", "grade.exposure.end", 0.75),
        ("driven", "grade.exposure.levels", 8.0),
        ("each", "each.looks[0].exposure", 0.6),
        ("each", "each.feather", 2.0),
        ("each", "each.seed", 5.0),
        ("region and frame", "region.near.low", 0.4),
        ("region and frame", "frame.angle", 400.0),
        ("two of one effect", "grade#2.exposure", 0.9),
    ];
    let all = recipes()?;
    for (kind, address, value) in rows {
        let recipe = &all.iter().find(|(name, _)| *name == kind).ok_or(kind)?.1;
        let before = recipe.numbers()?;
        let after = recipe.with_values(&[(address, value)], &Presets::new())?.numbers()?;
        let at = recipe.number(address)?.address;
        assert_eq!(before.len(), after.len(), "{kind}: {address}");
        for (was, now) in before.iter().zip(&after) {
            if was.address == at {
                assert_eq!(now.value, Some(value), "{kind}: {address}");
            } else {
                assert_eq!(was, now, "{kind}: {address} changed {}", was.address);
            }
        }
    }
    Ok(())
}

#[test]
fn several_numbers_are_set_together_and_a_rule_between_them_holds_for_the_set() -> Outcome {
    let recipe = &recipes()?[4].1; // band low 0.2, high 0.6
    let presets = Presets::new();
    let moved = recipe.with_values(
        &[
            ("region.near.low", 0.7),
            ("region.near.high", 0.9),
            ("grade.exposure", 0.5),
        ],
        &presets,
    )?;
    let value = |address: &str| moved.number(address).map(|number| number.value);
    assert_eq!(
        (
            value("region.near.low")?,
            value("region.near.high")?,
            value("grade.exposure")?
        ),
        (Some(0.7), Some(0.9), Some(0.5))
    );
    let said = refused(recipe.with_values(&[("region.near.low", 0.5), ("region.near.high", 0.3)], &presets));
    assert!(
        said.contains("'region.near.low', 'region.near.high'"),
        "neither is wrong alone, the pair is: {said}"
    );
    Ok(())
}

#[test]
fn nothing_is_set_unless_every_value_is_accepted() -> Outcome {
    let recipe = pulse();
    let before = recipe.numbers()?;
    let presets = Presets::new();
    for values in [
        [("grade.exposure", 0.5), ("grade.contrast", 99.0)],
        [("grade.contrast", 99.0), ("grade.exposure", 0.5)],
        [("grade.exposure", 0.5), ("grade.nope", 1.0)],
        [("grade.exposure", 0.5), ("grade.opacity", f64::NAN)],
    ] {
        let said = refused(recipe.with_values(&values, &presets));
        assert!(!said.is_empty(), "{values:?} was accepted");
        assert!(!said.contains("grade.exposure"), "a good value is blamed: {said}");
    }
    assert_eq!(recipe.numbers()?, before, "the recipe itself is left as it is");
    Ok(())
}

#[test]
fn a_value_out_of_its_range_is_refused_naming_the_address_the_range_and_the_value_never_clamped() -> Outcome {
    let contrast = pulse().number("grade.contrast")?;
    let presets = Presets::new();
    for beyond in [contrast.max + 1.0, contrast.min - 1.0] {
        let said = refused(pulse().with_values(&[("grade.contrast", beyond)], &presets));
        assert!(
            said.contains("'grade.contrast'") && said.contains("`contrast`") && said.contains(&format!("{beyond}")),
            "{said}"
        );
        assert!(
            said.contains(&format!("{}", contrast.max)) || said.contains(&format!("{}", contrast.min)),
            "the range is named: {said}"
        );
    }
    Ok(())
}

#[test]
fn a_whole_number_takes_a_whole_value_written_as_a_float_and_refuses_a_fraction() -> Outcome {
    let recipe = Recipe::new([Step::new("slice_shift")]);
    let presets = Presets::new();
    let set = recipe.with_values(&[("slice_shift.bands", 3.0)], &presets)?;
    assert_eq!(set.number("slice_shift.bands")?.value, Some(3.0));
    let said = refused(recipe.with_values(&[("slice_shift.bands", 3.5)], &presets));
    assert!(
        said.contains("'slice_shift.bands'") && said.contains("whole number") && said.contains("3.5"),
        "{said}"
    );
    Ok(())
}

#[test]
fn a_cyclic_angle_takes_any_value_and_two_addresses_of_one_number_are_refused() -> Outcome {
    let presets = Presets::new();
    let turned = pulse().with_values(&[("frame.angle", -725.0)], &presets)?;
    assert_eq!(turned.number("frame.angle")?.value, Some(-725.0));
    let named = Recipe::from_json(&json!({"steps": [{"effect": "grade", "name": "pulse"}]}).to_string())?;
    let said = refused(named.with_values(&[("pulse.exposure", 0.1), ("grade.exposure", 0.2)], &presets));
    assert!(
        said.contains("'pulse.exposure' and 'grade.exposure'") && said.contains("same number"),
        "{said}"
    );
    Ok(())
}

#[test]
fn an_unknown_address_among_good_ones_lists_what_is_valid_and_the_nearest() {
    let said = refused(pulse().with_values(&[("grade.exposure", 0.5), ("grade.exposur", 0.1)], &Presets::new()));
    assert!(
        said.contains("'grade.exposur'")
            && said.contains("its numbers:")
            && said.contains("did you mean grade.exposure"),
        "{said}"
    );
}

//! The ranges of the fields of a region, through the public API alone: `Recipe::checked` refuses a number outside its
//! range naming the region and the field, before it looks at the steps.

use bullterrier_image::region::{Band, Bands, Cuts, Grid, Line, LinearGradient, Shares, Svg};
use bullterrier_image::{Error, Presets, Recipe, Region, Step};
use serde_json::{Value, json};

/// The refusal of the recipe that declares `region` as `r` (and a grid `p`, to pick from) and has one step (no step
/// uses it), or None.
fn refusal(region: Region) -> Option<String> {
    let recipe = Recipe::new([Step::new("grade")])
        .with_region("p", Grid::default())
        .with_region("s", Svg::default())
        .with_region("r", region);
    match recipe.checked(&Presets::new()) {
        Err(Error::Recipe(message)) => Some(message),
        other => other.err().map(|error| error.to_string()),
    }
}

/// `kind` of the JSON `fields`, read as a region. A kind that must name regions names ones that exist (`p`, `depth`
/// and `subject`) unless the row says otherwise, so that only its numbers are in question.
fn region(kind: &str, fields: &Value) -> Region {
    let mut table = fields.clone();
    table["kind"] = json!(kind);
    let existing = match kind {
        "pick" => json!("p"),
        "band" | "bands" => json!("depth"),
        "combine" => json!(["subject", "depth"]),
        _ => Value::Null,
    };
    if !existing.is_null() && table.get("of").is_none() {
        table["of"] = existing;
    }
    serde_json::from_value(table).unwrap_or_else(|error| {
        Region::Grid(Grid {
            fit: Some(error.to_string()),
            ..Grid::default()
        })
    })
}

/// (the kind, the fields, what the refusal says; none: accepted). For each number the range, both ends and the first
/// value beyond each, written in the form `` `field` must be >= min, got value ``. The bounds are the Python
/// declarations' (`regions.py`), copied here by hand.
#[expect(clippy::too_many_lines, reason = "a table of rows, one bound each")]
fn rows() -> Vec<(&'static str, Value, Option<&'static str>)> {
    let edge = |field: &str, value: Value| json!({"edge": {field: value}});
    vec![
        // the borders of every laid kind: `grid` stands for them
        ("grid", edge("amount", json!(0)), None),
        ("grid", edge("amount", json!(48)), None),
        (
            "grid",
            edge("amount", json!(-0.1)),
            Some("`edge.amount` must be >= 0, got -0.1"),
        ),
        (
            "grid",
            edge("amount", json!(48.5)),
            Some("`edge.amount` must be <= 48, got 48.5"),
        ),
        ("grid", edge("scale", json!(1)), None),
        ("grid", edge("scale", json!(480)), None),
        (
            "grid",
            edge("scale", json!(0.5)),
            Some("`edge.scale` must be >= 1, got 0.5"),
        ),
        (
            "grid",
            edge("scale", json!(481)),
            Some("`edge.scale` must be <= 480, got 481"),
        ),
        ("grid", edge("octaves", json!(1)), None),
        ("grid", edge("octaves", json!(8)), None),
        (
            "grid",
            edge("octaves", json!(0)),
            Some("`edge.octaves` must be >= 1, got 0"),
        ),
        (
            "grid",
            edge("octaves", json!(9)),
            Some("`edge.octaves` must be <= 8, got 9"),
        ),
        ("grid", edge("persistence", json!(0)), None),
        ("grid", edge("persistence", json!(1)), None),
        (
            "grid",
            edge("persistence", json!(-0.25)),
            Some("`edge.persistence` must be >= 0, got -0.25"),
        ),
        (
            "grid",
            edge("persistence", json!(1.25)),
            Some("`edge.persistence` must be <= 1, got 1.25"),
        ),
        ("shards", edge("amount", json!(49)), Some("`edge.amount` must be <= 48")),
        ("cuts", edge("amount", json!(49)), Some("`edge.amount` must be <= 48")),
        ("path", edge("amount", json!(49)), Some("`edge.amount` must be <= 48")),
        ("svg", edge("amount", json!(49)), Some("`edge.amount` must be <= 48")),
        // grid: 1 to 1000 positive shares
        ("grid", json!({"columns": [1.0], "rows": [0.001]}), None),
        ("grid", json!({"columns": [5e99, 5e99]}), None),
        (
            "grid",
            json!({"columns": [1e308, 1e308]}),
            Some("`columns` shares must add up to at most 1e100"),
        ),
        (
            "grid",
            json!({"rows": [1e307, 1e307]}),
            Some("`rows` shares must add up to at most 1e100"),
        ),
        (
            "grid",
            json!({"rows": [6e99, 5e99]}),
            Some("`rows` shares must add up to at most 1e100"),
        ),
        ("grid", json!({"columns": vec![1; 1000]}), None),
        ("grid", json!({"rows": vec![1; 1000]}), None),
        (
            "grid",
            json!({"columns": vec![1; 1001]}),
            Some("`columns` must be a count or a list"),
        ),
        (
            "grid",
            json!({"rows": vec![1; 1001]}),
            Some("`rows` must be a count or a list"),
        ),
        (
            "grid",
            json!({"columns": []}),
            Some("`columns` must be a count or a list"),
        ),
        ("grid", json!({"rows": []}), Some("`rows` must be a count or a list")),
        (
            "grid",
            json!({"columns": [1, 0]}),
            Some("`columns` must be a count or a list"),
        ),
        (
            "grid",
            json!({"rows": [2, -1]}),
            Some("`rows` must be a count or a list"),
        ),
        // shards
        ("shards", json!({"count": 1}), None),
        ("shards", json!({"count": 500}), None),
        ("shards", json!({"count": 0}), Some("`count` must be >= 1, got 0")),
        ("shards", json!({"count": 501}), Some("`count` must be <= 500, got 501")),
        ("shards", json!({"jitter": 0}), None),
        ("shards", json!({"jitter": 1}), None),
        (
            "shards",
            json!({"jitter": -0.5}),
            Some("`jitter` must be >= 0, got -0.5"),
        ),
        ("shards", json!({"jitter": 1.5}), Some("`jitter` must be <= 1, got 1.5")),
        ("shards", json!({"impact": [0, 1]}), None),
        ("shards", json!({"impact": [1, 0]}), None),
        (
            "shards",
            json!({"impact": [-0.5, 0.5]}),
            Some("`impact[0]` must be >= 0, got -0.5"),
        ),
        (
            "shards",
            json!({"impact": [1.5, 0.5]}),
            Some("`impact[0]` must be <= 1, got 1.5"),
        ),
        (
            "shards",
            json!({"impact": [0.5, -0.5]}),
            Some("`impact[1]` must be >= 0, got -0.5"),
        ),
        (
            "shards",
            json!({"impact": [0.5, 1.5]}),
            Some("`impact[1]` must be <= 1, got 1.5"),
        ),
        ("shards", json!({"focus": 0}), None),
        ("shards", json!({"focus": 5}), None),
        ("shards", json!({"focus": -0.5}), Some("`focus` must be >= 0, got -0.5")),
        ("shards", json!({"focus": 5.5}), Some("`focus` must be <= 5, got 5.5")),
        // cuts: 1 to 32 of them; a line's point within the frame
        ("cuts", json!({"lines": vec![json!({}); 32]}), None),
        (
            "cuts",
            json!({"lines": vec![json!({}); 33]}),
            Some("`lines` allows at most 32 entries, got 33"),
        ),
        (
            "cuts",
            json!({"lines": []}),
            Some("`lines` needs at least 1 entries, got 0"),
        ),
        ("cuts", json!({"lines": [{"x": 0, "y": 1}]}), None),
        ("cuts", json!({"lines": [{"x": 1, "y": 0}]}), None),
        (
            "cuts",
            json!({"lines": [{"x": -0.5}]}),
            Some("`lines[0].x` must be >= 0, got -0.5"),
        ),
        (
            "cuts",
            json!({"lines": [{}, {"x": 1.5}]}),
            Some("`lines[1].x` must be <= 1, got 1.5"),
        ),
        (
            "cuts",
            json!({"lines": [{"y": -0.5}]}),
            Some("`lines[0].y` must be >= 0, got -0.5"),
        ),
        (
            "cuts",
            json!({"lines": [{"y": 1.5}]}),
            Some("`lines[0].y` must be <= 1, got 1.5"),
        ),
        ("cuts", json!({"lines": [{"angle": 725}, {"angle": -90}]}), None),
        ("cuts", json!({"lines": [{"path": "M 0 0 L 1 1"}]}), None),
        (
            "cuts",
            json!({"lines": [{"path": "M 0 0 L 1 1 M 0 1 L 1 0"}]}),
            Some("`lines[0].path` must be one path with a single M"),
        ),
        (
            "cuts",
            json!({"lines": [{}, {"path": "M 0 0 L foo"}]}),
            Some("`lines[1].path`: path data is not valid SVG path data"),
        ),
        (
            "cuts",
            json!({"lines": [{"path": ""}]}),
            Some("`lines[0].path`: path data is empty"),
        ),
        // path, svg
        ("path", json!({"samples": 1}), None),
        ("path", json!({"samples": 16}), None),
        ("path", json!({"samples": 0}), Some("`samples` must be >= 1, got 0")),
        ("path", json!({"samples": 17}), Some("`samples` must be <= 16, got 17")),
        (
            "path",
            json!({"d": "M 0.1 0.1 h 0.4 v 0.4 z M 0.6 0.6 c 0.1 0 0.2 0 0.3 0.1 Z"}),
            None,
        ),
        ("path", json!({"d": ""}), Some("`d`: path data is empty")),
        (
            "path",
            json!({"d": "L 0.5 0.5"}),
            Some("`d`: path data must start with M"),
        ),
        (
            "path",
            json!({"d": "M 0 0 L foo"}),
            Some("`d`: path data is not valid SVG path data"),
        ),
        (
            "path",
            json!({"d": "M 0 0 L 1"}),
            Some("`d`: path data is not valid SVG path data"),
        ),
        (
            "path",
            json!({"d": "M 0 0 L 1e999 0"}),
            Some("`d`: path data is not valid SVG path data"),
        ),
        (
            "path",
            json!({"d": "M 0 0 C 0 0 1e9 1e9 0 0"}),
            Some("`d`: a coordinate is not finite or reaches too far"),
        ),
        ("svg", json!({"max_shapes": 1, "max_kilobytes": 100_000}), None),
        ("svg", json!({"max_shapes": 100_000, "max_kilobytes": 1}), None),
        (
            "svg",
            json!({"max_shapes": 0}),
            Some("`max_shapes` must be >= 1, got 0"),
        ),
        (
            "svg",
            json!({"max_shapes": 100_001}),
            Some("`max_shapes` must be <= 100000, got 100001"),
        ),
        (
            "svg",
            json!({"max_kilobytes": 0}),
            Some("`max_kilobytes` must be >= 1, got 0"),
        ),
        (
            "svg",
            json!({"max_kilobytes": 100_001}),
            Some("`max_kilobytes` must be <= 100000, got 100001"),
        ),
        // pick: 1 to 10,000 pieces, and an id only of an svg
        ("pick", json!({"pieces": vec![0; 10_000]}), None),
        (
            "pick",
            json!({"pieces": vec![0; 10_001]}),
            Some("`pieces` allows at most 10000 entries, got 10001"),
        ),
        ("pick", json!({"of": "s", "pieces": ["logo", 2]}), None),
        (
            "pick",
            json!({"of": "p", "pieces": [0, "logo"]}),
            Some("`pieces` names the id 'logo', but 'p' is not an svg region"),
        ),
        (
            "pick",
            json!({"of": "depth", "pieces": ["logo"]}),
            Some("`pieces` names the id 'logo', but 'depth' is not an svg region"),
        ),
        ("pick", json!({"pieces": [0]}), None),
        (
            "pick",
            json!({"pieces": []}),
            Some("`pieces` needs at least 1 entries, got 0"),
        ),
        // grid: at most 10,000 pieces in all, whatever the shape
        ("grid", json!({"columns": 100, "rows": 100}), None),
        ("grid", json!({"columns": 1000, "rows": 10}), None),
        ("grid", json!({"columns": 10, "rows": 1000}), None),
        (
            "grid",
            json!({"columns": 101, "rows": 100}),
            Some("`columns` x `rows` make 10100 pieces, more than the limit of 10000"),
        ),
        (
            "grid",
            json!({"columns": 1000, "rows": 1000}),
            Some("`columns` x `rows` make 1000000 pieces, more than the limit of 10000"),
        ),
        (
            "grid",
            json!({"columns": vec![1; 1000], "rows": vec![2; 11]}),
            Some("make 11000 pieces"),
        ),
        // subject
        ("subject", json!({"context": 0, "threshold": 1}), None),
        ("subject", json!({"context": 1, "threshold": 0}), None),
        (
            "subject",
            json!({"context": -0.5}),
            Some("`context` must be >= 0, got -0.5"),
        ),
        (
            "subject",
            json!({"context": 1.5}),
            Some("`context` must be <= 1, got 1.5"),
        ),
        (
            "subject",
            json!({"threshold": -0.5}),
            Some("`threshold` must be >= 0, got -0.5"),
        ),
        (
            "subject",
            json!({"threshold": 1.5}),
            Some("`threshold` must be <= 1, got 1.5"),
        ),
        // band: three shares of the scale and low at most high
        ("band", json!({"low": 0, "high": 1, "feather": 0}), None),
        ("band", json!({"low": 1, "high": 1, "feather": 1}), None),
        ("band", json!({"low": 0.5, "high": 0.5}), None),
        ("band", json!({"low": -0.5}), Some("`low` must be >= 0, got -0.5")),
        (
            "band",
            json!({"low": 1.5, "high": 1}),
            Some("`low` must be <= 1, got 1.5"),
        ),
        (
            "band",
            json!({"high": -0.5, "low": 0}),
            Some("`high` must be >= 0, got -0.5"),
        ),
        ("band", json!({"high": 1.5}), Some("`high` must be <= 1, got 1.5")),
        (
            "band",
            json!({"feather": -0.5}),
            Some("`feather` must be >= 0, got -0.5"),
        ),
        ("band", json!({"feather": 1.5}), Some("`feather` must be <= 1, got 1.5")),
        (
            "band",
            json!({"low": 0.75, "high": 0.5}),
            Some("`low` 0.75 is above `high` 0.5"),
        ),
        (
            "band",
            json!({"low": 0.5, "high": 0.4999}),
            Some("`low` 0.5 is above `high` 0.4999"),
        ),
        // bands, combine: 2 to 1000 regions
        ("combine", json!({"of": vec!["subject"; 1000]}), None),
        (
            "combine",
            json!({"of": vec!["subject"; 1001]}),
            Some("`of` allows at most 1000 entries, got 1001"),
        ),
        ("bands", json!({"count": 1}), None),
        ("bands", json!({"count": 100}), None),
        ("bands", json!({"count": 0}), Some("`count` must be >= 1, got 0")),
        ("bands", json!({"count": 101}), Some("`count` must be <= 100, got 101")),
        ("combine", json!({"of": ["subject", "depth"]}), None),
        ("combine", json!({"of": ["subject", "depth", "light"]}), None),
        (
            "combine",
            json!({"of": ["subject"]}),
            Some("`of` needs at least 2 entries, got 1"),
        ),
        (
            "combine",
            json!({"of": []}),
            Some("`of` needs at least 2 entries, got 0"),
        ),
        // gradients
        ("linear_gradient", json!({"angle": 0}), None),
        ("linear_gradient", json!({"angle": 360}), None),
        ("linear_gradient", json!({"angle": 725}), None),
        ("linear_gradient", json!({"angle": -90}), None),
        ("radial_gradient", json!({"x": 0, "y": 1}), None),
        ("radial_gradient", json!({"x": 1, "y": 0}), None),
        (
            "radial_gradient",
            json!({"x": -0.5}),
            Some("`x` must be >= 0, got -0.5"),
        ),
        ("radial_gradient", json!({"x": 1.5}), Some("`x` must be <= 1, got 1.5")),
        (
            "radial_gradient",
            json!({"y": -0.5}),
            Some("`y` must be >= 0, got -0.5"),
        ),
        ("radial_gradient", json!({"y": 1.5}), Some("`y` must be <= 1, got 1.5")),
        ("radial_gradient", json!({"radius": 0.01}), None),
        ("radial_gradient", json!({"radius": 3}), None),
        (
            "radial_gradient",
            json!({"radius": 0.005}),
            Some("`radius` must be >= 0.01, got 0.005"),
        ),
        (
            "radial_gradient",
            json!({"radius": 3.5}),
            Some("`radius` must be <= 3, got 3.5"),
        ),
        ("given", json!({}), None),
    ]
}

#[test]
fn every_number_of_a_region_lies_within_its_range_both_ends_included() {
    for (kind, fields, said) in rows() {
        let refused = refusal(region(kind, &fields));
        // a row may hold thousands of entries: show the start of it
        let shown: String = fields.to_string().chars().take(100).collect();
        match said {
            None => assert_eq!(refused, None, "{kind} {shown} is within its ranges"),
            Some(said) => assert!(
                refused
                    .as_deref()
                    .is_some_and(|message| message.starts_with("region 'r': ") && message.contains(said)),
                "{kind} {shown}: wanted {said:?}, got {refused:?}"
            ),
        }
    }
}

#[test]
fn a_number_that_is_not_finite_is_refused_where_a_file_cannot_write_it() {
    let nan = f64::NAN;
    for (what, region, field) in [
        (
            "a share that is NaN",
            Region::Grid(Grid {
                columns: Shares(vec![1.0, nan]),
                ..Grid::default()
            }),
            "columns",
        ),
        (
            "an infinite share",
            Region::Grid(Grid {
                rows: Shares(vec![f64::INFINITY]),
                ..Grid::default()
            }),
            "rows",
        ),
        (
            "a band that is NaN",
            Region::Band(Band {
                low: nan,
                ..Band::default()
            }),
            "low",
        ),
        (
            "a feather that is infinite",
            Region::Band(Band {
                feather: f64::INFINITY,
                ..Band::default()
            }),
            "feather",
        ),
        (
            "an angle that is NaN in a line",
            Region::Cuts(Cuts {
                lines: vec![
                    Line {
                        angle: nan,
                        ..Line::default()
                    }
                    .into(),
                ],
                ..Cuts::default()
            }),
            "lines[0].angle",
        ),
        (
            "an infinite gradient angle",
            Region::LinearGradient(LinearGradient { angle: f64::INFINITY }),
            "angle",
        ),
    ] {
        let refused = refusal(region);
        assert!(
            refused.as_deref().is_some_and(
                |message| message.contains(field) && (message.contains("finite") || message.contains("positive"))
            ),
            "{what}: {refused:?}"
        );
    }
}

#[test]
fn the_regions_are_checked_before_the_steps_and_whether_or_not_a_step_uses_them() {
    let bad = Region::Bands(Bands {
        count: 0,
        ..Bands::default()
    });
    // the step's effect does not exist either, and the region is named first
    let recipe = Recipe::new([Step::new("nope")]).with_region("unused", bad);
    let refused = recipe.checked(&Presets::new()).err().map(|error| error.to_string());
    assert!(
        refused
            .as_deref()
            .is_some_and(|message| message.contains("region 'unused'")),
        "{refused:?}"
    );
}

#[test]
fn of_several_regions_that_break_a_rule_the_first_declared_is_named() {
    let bands = |count| {
        Region::Bands(Bands {
            count,
            ..Bands::default()
        })
    };
    let recipe = Recipe::new([Step::new("grade")])
        .with_region("zed", bands(0))
        .with_region("amy", bands(101));
    let refused = recipe.checked(&Presets::new()).err().map(|error| error.to_string());
    assert!(
        refused
            .as_deref()
            .is_some_and(|message| message.contains("region 'zed'")),
        "{refused:?}"
    );
}

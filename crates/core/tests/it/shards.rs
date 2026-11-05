//! `shards` regions, seen the way an effect sees them: through the public API alone, with an effect of the test's own that
//! logs the partition it is handed; and the presets a shard region can be read with.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use bullterrier_image::region::{Edge, EdgeStyle, Shards};
use bullterrier_image::{
    Category, Effect, Error, Image, Input, Inputs, Options, Photo, Picture, Presets, Recipe, Region, Step,
    region_presets, register,
};
use serde_json::{Value, json};

use crate::common::compare::first_difference;

/// The partition a [`Spy`] was handed: its labels and number of pieces.
type Pieces = (Vec<i32>, usize);

/// An effect that logs the partition it is handed and returns the picture as it is.
struct Spy {
    name: String,
    log: Mutex<Vec<Pieces>>,
}

impl Effect for Spy {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn inputs(&self) -> &[Input] {
        &[Input::Partition]
    }

    fn apply(&self, picture: &Picture<'_>, _params: serde_json::Value, inputs: &Inputs<'_>) -> Result<Image, Error> {
        if let Some(labels) = inputs.partition() {
            self.log
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((labels.values().to_vec(), labels.pieces()));
        }
        Ok(picture.map(|pixel| pixel))
    }
}

/// The labels and number of pieces of the shards `region` on a grey photo of `height` x `width` pixels.
fn pieces(height: usize, width: usize, region: impl Into<Region>) -> Result<Pieces, Error> {
    static SPIES: AtomicUsize = AtomicUsize::new(0);
    let spy = Arc::new(Spy {
        name: format!("shards_spy_{}", SPIES.fetch_add(1, Ordering::Relaxed)),
        log: Mutex::new(Vec::new()),
    });
    register(spy.clone())?;
    let pixels = vec![0.5; height * width * 3];
    let photo = Photo::new(Picture::new(height, width, &pixels)?);
    let mut step = Step::new(&spy.name);
    step.partition = Some("s".to_owned());
    let recipe = Recipe::new([step]).with_region("s", region);
    photo.render(&recipe, &Presets::new(), Options::default())?;
    let mut seen = spy.log.lock().unwrap_or_else(PoisonError::into_inner);
    seen.pop()
        .ok_or_else(|| Error::Recipe("the spy was handed no partition".to_owned()))
}

fn shards(count: u32, jitter: f64, seed: u32) -> Shards {
    Shards {
        count,
        jitter,
        seed,
        ..Shards::default()
    }
}

/// A number of pieces as a label bound, saturating.
fn bound(count: usize) -> i32 {
    i32::try_from(count).unwrap_or(i32::MAX)
}

/// Whether the labels are the pieces' numbers in reading order: each label first appears right after the greatest before it,
/// and all `count` appear.
fn numbered_by_first_pixel(labels: &[i32], count: usize) -> bool {
    let mut greatest = -1;
    for &label in labels {
        if label > greatest + 1 || !(0..bound(count)).contains(&label) {
            return false;
        }
        greatest = greatest.max(label);
    }
    greatest + 1 == bound(count)
}

#[test]
fn one_shard_is_the_whole_picture() -> Result<(), Error> {
    let (labels, count) = pieces(48, 64, shards(1, 0.5, 0))?;
    assert_eq!((count, labels.iter().all(|&label| label == 0)), (1, true));
    Ok(())
}

#[test]
fn a_regular_honeycomb_on_a_big_picture_has_one_piece_for_each_seed() -> Result<(), Error> {
    for count in [2, 12, 70, 500] {
        let (_, pieces) = pieces(240, 320, shards(count, 0.0, 3))?;
        assert_eq!(pieces, usize::try_from(count).unwrap_or(0), "{count} seeds");
    }
    Ok(())
}

#[test]
fn a_picture_too_small_to_give_each_seed_a_pixel_has_fewer_pieces() -> Result<(), Error> {
    let (labels, count) = pieces(2, 3, shards(50, 0.5, 1))?;
    assert!((1..=6).contains(&count), "{count}");
    assert!(numbered_by_first_pixel(&labels, count));
    // the largest count on a modest picture
    assert!((1..=500).contains(&pieces(48, 64, shards(500, 1.0, 1))?.1));
    Ok(())
}

#[test]
fn the_pieces_partition_the_picture_and_are_numbered_by_first_pixel() -> Result<(), Error> {
    let impact = [0.2, 0.9];
    for region in [
        shards(12, 0.5, 0),
        Shards {
            count: 40,
            focus: 2.5,
            impact,
            ..Shards::default()
        },
        Shards {
            count: 9,
            jitter: 1.0,
            edge: Edge {
                style: EdgeStyle::Torn,
                amount: 12.0,
                scale: 24.0,
                ..Edge::default()
            },
            ..Shards::default()
        },
    ] {
        let (labels, count) = pieces(96, 128, region)?;
        assert_eq!(labels.len(), 96 * 128);
        assert!(numbered_by_first_pixel(&labels, count));
    }
    Ok(())
}

#[test]
fn the_same_seed_cuts_the_same_shards_and_another_seed_other_ones() -> Result<(), Error> {
    let first = pieces(96, 128, shards(12, 0.5, 1))?;
    let again = pieces(96, 128, shards(12, 0.5, 1))?;
    let other = pieces(96, 128, shards(12, 0.5, 2))?;
    assert_eq!(first_difference(&first.0, &again.0), None);
    assert!(first_difference(&first.0, &other.0).is_some());
    Ok(())
}

#[test]
fn torn_shards_are_the_straight_ones_bent_by_the_seed() -> Result<(), Error> {
    let torn = |seed, amount| Shards {
        edge: Edge {
            style: EdgeStyle::Torn,
            amount,
            scale: 24.0,
            seed,
            octaves: 1,
            ..Edge::default()
        },
        ..shards(6, 0.7, 4)
    };
    let straight = pieces(96, 128, shards(6, 0.7, 4))?;
    let nothing = pieces(96, 128, torn(1, 0.0))?;
    let first = pieces(96, 128, torn(1, 12.0))?;
    let again = pieces(96, 128, torn(1, 12.0))?;
    let other = pieces(96, 128, torn(2, 12.0))?;
    assert_eq!(first_difference(&straight.0, &nothing.0), None);
    assert_eq!(first_difference(&first.0, &again.0), None);
    assert!(first_difference(&straight.0, &first.0).is_some());
    assert!(first_difference(&first.0, &other.0).is_some());
    Ok(())
}

// --- the presets ---------------------------------------------------------------------------------------------------

/// The shipped presets of the shards kind, read from the file itself: each name with the table of its fields. A missing
/// or broken file fails the test that asks.
fn shipped() -> Result<Vec<(String, Value)>, String> {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/regions/shards.toml");
    let text = std::fs::read_to_string(&file).map_err(|error| format!("{}: {error}", file.display()))?;
    match toml::from_str::<Value>(&text).map_err(|error| error.to_string())? {
        Value::Object(table) => Ok(table.into_iter().collect()),
        other => Err(format!("not a table of presets: {other}")),
    }
}

/// The shards region `fields` (the `preset` and what is given beside it) is read as, or the message that refuses it.
fn read(fields: &Value) -> Result<Shards, String> {
    let mut region = fields.clone();
    region["kind"] = json!("shards");
    let recipe = Recipe::from_json(&json!({"steps": [], "regions": {"s": region}}).to_string())
        .map_err(|error| error.to_string())?;
    match recipe.regions.get("s") {
        Some(Region::Shards(shards)) => Ok(shards.clone()),
        other => Err(format!("not a shards region: {other:?}")),
    }
}

/// The fields of `shards` as a JSON object.
fn written(shards: &Shards) -> Value {
    serde_json::to_value(shards).unwrap_or(Value::Null)
}

#[test]
fn the_shipped_presets_are_glass_torn_stone_and_mosaic() -> Result<(), String> {
    let names: Vec<String> = shipped()?.into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["glass", "torn", "stone", "mosaic"]);
    Ok(())
}

#[test]
fn region_presets_lists_the_four_with_their_sentence_and_fields_in_file_order() -> Result<(), Error> {
    let found = region_presets("shards")?;
    let names: Vec<&str> = found.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["glass", "torn", "stone", "mosaic"]);
    let described: Vec<(&str, Value)> = found
        .iter()
        .map(|(_, preset)| (preset.description.as_str(), Value::Object(preset.params.clone())))
        .collect();
    let expected = [
        (
            "Dozens of splinters crowded round an impact point in the middle, big pieces far out, straight cracks.",
            json!({"count": 46, "jitter": 1.0, "focus": 2.5}),
        ),
        (
            "A few big pieces with wide ragged edges, like paper torn apart by hand.",
            json!({"count": 6, "jitter": 0.7, "edge": {"style": "torn", "amount": 5.76, "scale": 33.6}}),
        ),
        (
            "About a dozen cracked slabs with slightly rough edges, like a broken pavement.",
            json!({"count": 14, "jitter": 0.8, "edge": {"style": "torn", "amount": 2.4, "scale": 8.64}}),
        ),
        (
            "Many small, nearly even tiles with straight edges, like a mosaic of cut glass.",
            json!({"count": 70, "jitter": 0.25}),
        ),
    ];
    assert_eq!(described, expected);
    Ok(())
}

#[test]
fn region_presets_of_a_kind_without_presets_names_the_kinds_that_have_them() {
    let refused = region_presets("grid")
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(refused.contains("'grid'") && refused.contains("shards"), "{refused}");
}

#[test]
fn a_preset_sets_every_field_its_file_gives_and_leaves_the_others_at_their_defaults() -> Result<(), String> {
    let defaults = written(&Shards::default());
    for (name, fields) in shipped()? {
        let made = written(&read(&json!({"preset": name}))?);
        for (field, value) in fields.as_object().into_iter().flatten() {
            match field.as_str() {
                "description" => assert_eq!(made.get("description"), None, "{name}"),
                // the file may name only some of an edge's fields
                "edge" => {
                    for (key, value) in value.as_object().into_iter().flatten() {
                        assert_eq!(made["edge"].get(key), Some(value), "{name}.edge.{key}");
                    }
                }
                _ => assert_eq!(made.get(field), Some(value), "{name}.{field}"),
            }
        }
        for field in ["jitter", "impact", "focus", "seed", "count"] {
            if fields.get(field).is_none() {
                assert_eq!(made[field], defaults[field], "{name}.{field}");
            }
        }
        assert_eq!(made["preset"], json!(name));
    }
    Ok(())
}

#[test]
fn a_field_given_beside_a_preset_wins_and_a_given_edge_replaces_the_presets_whole_edge() -> Result<(), String> {
    // glass: count 46, jitter 1, focus 2.5
    let glass = read(&json!({"preset": "glass", "count": 3}))?;
    assert_eq!((glass.count, glass.jitter, glass.focus), (3, 1.0, 2.5));
    // torn has a torn edge of amount 5.76; a given edge is not merged into it
    let preset = read(&json!({"preset": "torn"}))?;
    assert_eq!(preset.edge.style, EdgeStyle::Torn);
    let given = read(&json!({"preset": "torn", "edge": {"amount": 1.0}}))?;
    assert_eq!((given.edge.style, given.edge.amount), (EdgeStyle::Straight, 1.0));
    assert_eq!(given.count, preset.count);
    Ok(())
}

#[test]
fn a_preset_is_kept_as_written_and_reading_what_was_written_changes_nothing() -> Result<(), String> {
    let stone = read(&json!({"preset": "stone"}))?;
    assert_eq!(stone.preset.as_deref(), Some("stone"));
    assert_eq!(read(&written(&stone))?, stone);
    Ok(())
}

#[test]
fn an_unknown_preset_is_refused_listing_the_four() {
    let refused = read(&json!({"preset": "shattered"})).err().unwrap_or_default();
    for word in ["shattered", "glass", "torn", "stone", "mosaic"] {
        assert!(refused.contains(word), "{refused}");
    }
}

#[test]
fn the_values_of_a_preset_are_checked_like_any_others() {
    let recipe = Recipe::new([Step::new("grade")]);
    let shards = read(&json!({"preset": "glass", "count": 501})).unwrap_or_default();
    let refused = recipe
        .with_region("s", shards)
        .checked(&Presets::new())
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(refused.contains("count"), "{refused}");
}

#[test]
fn a_preset_cuts_the_shards_of_its_values() -> Result<(), String> {
    // mosaic: 70 tiles of nearly even size on a picture big enough for each, glass: crowded round the middle
    let tiles = read(&json!({"preset": "mosaic"}))?;
    let (_, count) = pieces(240, 320, tiles).map_err(|error| error.to_string())?;
    assert!((60..=70).contains(&count), "{count}");
    Ok(())
}

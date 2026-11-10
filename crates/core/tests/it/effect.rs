//! What an effect is handed, through the public API alone: the inputs it declares arrive, those it does not are absent,
//! and a run with an input missing or of the wrong size is refused before the effect runs.

use std::collections::BTreeMap;

use crate::common::Outcome;
use crate::common::run::{Counting, Recorder, Registered};
use bullterrier_image::{Category, Error, Input, Inputs, Labels, Layer, Picture, Place, apply};
use serde_json::json;

/// A 2 x 3 picture and the partition and map to offer with it.
fn picture_values() -> Vec<f32> {
    vec![0.5; 2 * 3 * 3]
}

fn labels() -> Result<Labels, Error> {
    Labels::new(2, 3, vec![0, 0, 1, 1, 2, 2], 3)
}

fn layer(level: f32) -> Result<Layer, Error> {
    Layer::new(2, 3, vec![level; 6])
}

const PARTITION: &[Input] = &[Input::Partition];
const RATIO: &[Input] = &[Input::SizeRatio];
const PLACE: &[Input] = &[Input::Place];

#[test]
fn a_declared_partition_arrives_and_an_undeclared_one_is_absent() -> Outcome {
    let (values, labels) = (picture_values(), labels()?);
    let picture = Picture::new(2, 3, &values)?;
    let offered = Inputs::default().with_partition(&labels);
    let takes = Registered::new(Recorder::new(PARTITION, &[]))?;
    let ignores = Registered::new(Recorder::new(&[], &[]))?;
    apply(takes.name(), &picture, json!({}), &offered)?;
    apply(ignores.name(), &picture, json!({}), &offered)?;
    assert_eq!(takes.received()[0].partition, Some(vec![0, 0, 1, 1, 2, 2]));
    assert_eq!(ignores.received()[0].partition, None);
    Ok(())
}

#[test]
fn a_declared_field_arrives_under_its_parameter_name_and_an_undeclared_one_is_absent() -> Outcome {
    let (values, depth, light) = (picture_values(), layer(0.25)?, layer(0.75)?);
    let picture = Picture::new(2, 3, &values)?;
    let offered = Inputs::default()
        .with_field("depth", &depth)
        .with_field("light", &light);
    let takes_depth = Registered::new(Recorder::new(&[], &["depth"]))?;
    let takes_none = Registered::new(Recorder::new(&[], &[]))?;
    apply(takes_depth.name(), &picture, json!({}), &offered)?;
    apply(takes_none.name(), &picture, json!({}), &offered)?;
    // exactly the declared one, under the parameter's name, with that map's values
    assert_eq!(
        takes_depth.received()[0].fields,
        BTreeMap::from([("depth".to_owned(), vec![0.25; 6])])
    );
    assert!(takes_none.received()[0].fields.is_empty());
    Ok(())
}

#[test]
fn the_size_ratio_arrives_only_if_declared_else_the_effect_sees_one() -> Outcome {
    let values = picture_values();
    let picture = Picture::new(2, 3, &values)?;
    let offered = Inputs::default().with_size_ratio(2.5);
    let takes = Registered::new(Recorder::new(RATIO, &[]))?;
    let ignores = Registered::new(Recorder::new(&[], &[]))?;
    apply(takes.name(), &picture, json!({}), &offered)?;
    apply(ignores.name(), &picture, json!({}), &offered)?;
    assert!((takes.received()[0].size_ratio - 2.5).abs() < f64::EPSILON);
    assert!((ignores.received()[0].size_ratio - 1.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn the_place_arrives_only_if_declared_else_the_canvas_is_the_picture_itself() -> Outcome {
    let values = picture_values();
    let picture = Picture::new(2, 3, &values)?;
    let offered = Inputs::default().with_place(Place::new(10, 3, 4));
    let takes = Registered::new(Recorder::new(PLACE, &[]))?;
    let ignores = Registered::new(Recorder::new(&[], &[]))?;
    let unoffered = Registered::new(Recorder::new(PLACE, &[]))?;
    apply(takes.name(), &picture, json!({}), &offered)?;
    apply(ignores.name(), &picture, json!({}), &offered)?;
    apply(unoffered.name(), &picture, json!({}), &Inputs::default())?;
    assert_eq!(takes.received()[0].place, Place::new(10, 3, 4));
    assert_eq!(ignores.received()[0].place, Place::new(2, 3, 0));
    assert_eq!(unoffered.received()[0].place, Place::new(2, 3, 0));
    Ok(())
}

#[test]
fn a_place_that_does_not_hold_the_picture_refuses_the_run_naming_the_effect() -> Outcome {
    let values = picture_values();
    let picture = Picture::new(2, 3, &values)?;
    let takes = Registered::new(Recorder::new(PLACE, &[]))?;
    // (the place offered, what the message must say): rows 4..6 of a canvas of 5 rows, a canvas of another width;
    // rows 3..5 of 5 and the first row of a canvas of exactly the picture's size are fine
    for (place, said) in [
        (Place::new(5, 3, 4), "rows 4..6"),
        (Place::new(10, 4, 0), "canvas is 4 wide"),
    ] {
        let result = apply(takes.name(), &picture, json!({}), &Inputs::default().with_place(place));
        assert!(
            matches!(&result, Err(error @ Error::Inputs(_)) if error.to_string().contains(takes.name()) && error.to_string().contains(said)),
            "{place:?}: {result:?}"
        );
    }
    assert_eq!(takes.received().len(), 0);
    for place in [Place::new(5, 3, 3), Place::new(2, 3, 0)] {
        apply(takes.name(), &picture, json!({}), &Inputs::default().with_place(place))?;
    }
    assert_eq!(takes.received().len(), 2);
    Ok(())
}

#[test]
fn a_declared_input_that_is_not_on_offer_refuses_the_run_with_a_hint_for_its_kind_and_the_effect_never_runs() -> Outcome
{
    let values = picture_values();
    let picture = Picture::new(2, 3, &values)?;
    let counting = Registered::new(Counting::new())?;
    let field = Registered::new(Recorder::new(&[], &["depth"]))?;
    // (the effect, the input it lacks, the hint of its kind)
    let rows = [
        (counting.name(), "partition", "set the step's region"),
        (field.name(), "depth", "name a map or region in the step's `depth`"),
    ];
    for (name, input, hint) in rows {
        let result = apply(name, &picture, json!({}), &Inputs::default());
        assert!(
            matches!(&result, Err(error @ Error::Inputs(_)) if error.to_string().contains(input) && error.to_string().contains(hint)),
            "{input}: {result:?}"
        );
    }
    assert_eq!((counting.calls(), field.received().len()), (0, 0));
    Ok(())
}

#[test]
fn a_partition_or_map_of_another_size_than_the_picture_refuses_the_run() -> Outcome {
    let values = picture_values();
    let picture = Picture::new(2, 3, &values)?;
    let (small_labels, small_layer) = (Labels::new(1, 3, vec![0; 3], 1)?, Layer::new(2, 2, vec![0.5; 4])?);
    let partition = Registered::new(Recorder::new(PARTITION, &[]))?;
    let field = Registered::new(Recorder::new(&[], &["depth"]))?;
    let result = apply(
        partition.name(),
        &picture,
        json!({}),
        &Inputs::default().with_partition(&small_labels),
    );
    assert!(
        matches!(&result, Err(error @ Error::Inputs(_)) if error.to_string().contains("1 x 3")),
        "{result:?}"
    );
    let result = apply(
        field.name(),
        &picture,
        json!({}),
        &Inputs::default().with_field("depth", &small_layer),
    );
    assert!(
        matches!(&result, Err(error @ Error::Inputs(_)) if error.to_string().contains("2 x 2")),
        "{result:?}"
    );
    assert_eq!((partition.received().len(), field.received().len()), (0, 0));
    Ok(())
}

#[test]
fn all_categories_lists_every_variant_once_in_catalogue_order() {
    // the catalogue's sections in the order it lists them, written out by hand
    let catalogue = [
        "colour",
        "light",
        "texture",
        "stylised",
        "text",
        "distortion",
        "glitch",
        "atmosphere",
        "subject",
        "compositing",
        "backgrounds",
        "utilities",
    ];
    let names: Vec<&str> = Category::ALL.iter().map(|category| category.name()).collect();
    assert_eq!(names, catalogue);
}

#[test]
fn all_inputs_lists_every_variant_once_in_recipe_order() {
    let names: Vec<&str> = Input::ALL.iter().map(|input| input.name()).collect();
    assert_eq!(names, ["partition", "size_ratio", "place"]);
}

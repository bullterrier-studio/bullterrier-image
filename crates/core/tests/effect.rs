//! What an effect is handed, through the public API alone: the inputs it declares arrive, those it does not are absent,
//! and a run with an input missing or of the wrong size is refused before the effect runs.

mod common;

use std::collections::BTreeMap;

use bullterrier_image::{Category, Error, Input, Inputs, Labels, Layer, Picture, apply};
use common::Outcome;
use common::run::{Counting, Recorder, Registered};
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
fn a_declared_input_that_is_not_on_offer_refuses_the_run_and_the_effect_never_runs() -> Outcome {
    let values = picture_values();
    let picture = Picture::new(2, 3, &values)?;
    let counting = Registered::new(Counting::new())?;
    let result = apply(counting.name(), &picture, json!({}), &Inputs::default());
    assert!(
        matches!(&result, Err(error @ Error::Inputs(_)) if error.to_string().contains("partition")),
        "{result:?}"
    );
    let field = Registered::new(Recorder::new(&[], &["depth"]))?;
    let result = apply(field.name(), &picture, json!({}), &Inputs::default());
    assert!(
        matches!(&result, Err(error @ Error::Inputs(_)) if error.to_string().contains("depth")),
        "{result:?}"
    );
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
    assert_eq!(names, ["partition", "size_ratio"]);
}

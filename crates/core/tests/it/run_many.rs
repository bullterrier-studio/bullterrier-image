//! Several recipes on one photo: each map found once however many recipes need it, and the names given held to the rules of
//! the recipes.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bullterrier_image::{Error, ErrorKind, Finder, Layer, Options, Photo, Picture, Presets, Recipe, Step};

use crate::common::Outcome;

/// A finder that says the left half is the subject, and counts how often it is asked.
struct Halves(Arc<AtomicUsize>);

impl Finder for Halves {
    fn find(&self, picture: &Picture<'_>) -> Result<Layer, Error> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let (height, width) = (picture.height(), picture.width());
        Layer::new(
            height,
            width,
            (0..height * width)
                .map(|i| f32::from(u8::from(i % width < width / 2)))
                .collect(),
        )
    }
}

fn masked(exposure: f64) -> Recipe {
    Recipe::new([Step::new("grade").param("exposure", exposure).mask("subject")])
}

fn plain() -> Recipe {
    Recipe::new([Step::new("grade").param("exposure", 0.5)])
}

const PIXELS: [f32; 8 * 8 * 3] = [0.5; 8 * 8 * 3];

fn finders(asked: &Arc<AtomicUsize>) -> BTreeMap<String, Arc<dyn Finder>> {
    BTreeMap::from([(
        "subject".to_owned(),
        Arc::new(Halves(Arc::clone(asked))) as Arc<dyn Finder>,
    )])
}

#[test]
fn the_subject_is_found_once_for_all_the_recipes_that_need_it() -> Outcome {
    let (asked, picture) = (Arc::new(AtomicUsize::new(0)), Picture::new(8, 8, &PIXELS)?);
    let recipes = [masked(0.2), plain(), masked(0.4)];
    let photos = Photo::for_recipes(picture, &recipes, &Presets::new(), BTreeMap::new(), &finders(&asked))?;
    assert_eq!(photos.len(), 3);
    for (photo, recipe) in photos.iter().zip(&recipes) {
        photo.render(recipe, &Presets::new(), Options::default())?;
    }
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn a_finder_nobody_needs_is_not_asked_and_a_map_handed_in_is_not_found() -> Outcome {
    let (asked, picture) = (Arc::new(AtomicUsize::new(0)), Picture::new(8, 8, &PIXELS)?);
    Photo::for_recipes(picture, &[plain()], &Presets::new(), BTreeMap::new(), &finders(&asked))?;
    assert_eq!(asked.load(Ordering::SeqCst), 0, "no recipe uses the subject");
    let handed = BTreeMap::from([("subject".to_owned(), Layer::new(8, 8, vec![1.0; 64])?)]);
    Photo::for_recipes(picture, &[masked(0.2)], &Presets::new(), handed, &finders(&asked))?;
    assert_eq!(asked.load(Ordering::SeqCst), 0, "a map handed in is used as it is");
    Ok(())
}

#[test]
fn a_map_nobody_can_give_is_the_maps_error_and_a_name_no_recipe_knows_lists_the_names() -> Outcome {
    let picture = Picture::new(8, 8, &PIXELS)?;
    let missing = Photo::for_recipes(
        picture,
        &[masked(0.2)],
        &Presets::new(),
        BTreeMap::new(),
        &BTreeMap::new(),
    );
    assert!(
        matches!(&missing, Err(error) if error.kind() == ErrorKind::SubjectNotFound),
        "{missing:?}"
    );
    let handed = BTreeMap::from([("sky".to_owned(), Layer::new(8, 8, vec![1.0; 64])?)]);
    let unknown = Photo::for_recipes(picture, &[masked(0.2)], &Presets::new(), handed, &BTreeMap::new());
    let text = unknown.err().map(|error| error.to_string()).unwrap_or_default();
    assert!(text.contains("\"sky\"") && text.contains("depth, subject"), "{text}");
    Ok(())
}

#[test]
fn a_map_handed_in_of_the_wrong_size_is_refused_even_if_no_recipe_uses_it() -> Outcome {
    let picture = Picture::new(8, 8, &PIXELS)?;
    let handed = BTreeMap::from([("subject".to_owned(), Layer::new(4, 4, vec![1.0; 16])?)]);
    let refused = Photo::for_recipes(picture, &[plain()], &Presets::new(), handed, &BTreeMap::new());
    assert!(
        matches!(&refused, Err(error) if error.kind() == ErrorKind::SubjectNotFound),
        "{:?}",
        refused.err()
    );
    Ok(())
}

//! A recipe's work before its first pixel grows with its size, not with its size squared: a chain of 2,000 regions used by
//! 2,000 steps is asked what it needs, rendered and traced in a moment.
//!
//! This is the one test that looks at a clock, and only with a bound far above the cost: the chain below took 26 s to ask
//! `needs` in the test profile (4.5 s in the extension's debug build) when every step walked every region and every
//! lookup scanned them, and takes about 0.2 s for the three calls now. A slow machine cannot fail it; a machine fast
//! enough to pass the old code in 3 s could, which is why the bound is generous and the picture is 4 x 4 pixels.

use std::error::Error;
use std::time::{Duration, Instant};

use bullterrier_image::{Layer, Options, Photo, Picture, Presets, Recipe};
use serde_json::json;

/// Regions in the chain, and steps that each use the last of them.
const LINKS: usize = 2_000;
/// What each of the three calls may take.
const AT_MOST: Duration = Duration::from_secs(3);

fn recipe() -> Result<Recipe, Box<dyn Error>> {
    let mut regions = serde_json::Map::new();
    for link in 0..LINKS {
        let of = if link == 0 {
            "depth".to_owned()
        } else {
            format!("r{}", link - 1)
        };
        regions.insert(
            format!("r{link}"),
            json!({"kind": "band", "of": of, "low": 0.0, "high": 1.0, "feather": 0.0}),
        );
    }
    let step = json!({"effect": "grade", "params": {"exposure": 0.1}, "mask": format!("r{}", LINKS - 1)});
    let steps = vec![step; LINKS];
    Ok(Recipe::from_json(
        &json!({"steps": steps, "regions": regions}).to_string(),
    )?)
}

/// `call`'s result, and an error if it took longer than [`AT_MOST`].
fn quickly<T>(what: &str, call: impl FnOnce() -> Result<T, Box<dyn Error>>) -> Result<T, Box<dyn Error>> {
    let started = Instant::now();
    let result = call()?;
    let took = started.elapsed();
    assert!(took < AT_MOST, "{what} took {took:?}");
    Ok(result)
}

#[test]
fn a_long_chain_of_regions_used_by_as_many_steps_is_asked_rendered_and_traced_in_a_moment() -> Result<(), Box<dyn Error>>
{
    let recipe = recipe()?;
    let needs = quickly("needs", || Ok(recipe.needs(&Presets::new())?))?;
    assert_eq!(needs.maps.iter().map(String::as_str).collect::<Vec<_>>(), ["depth"]);
    let rgb = vec![0.5_f32; 4 * 4 * 3];
    let photo = Photo::new(Picture::new(4, 4, &rgb)?).with_map("depth", Layer::new(4, 4, vec![0.5; 16])?)?;
    let rendered = quickly("render", || {
        Ok(photo.render(&recipe, &Presets::new(), Options::default())?)
    })?;
    assert_eq!((rendered.height(), rendered.width()), (4, 4));
    let stages = quickly("trace", || {
        Ok(photo.trace(&recipe, &Presets::new(), Options::default())?)
    })?;
    assert_eq!(stages.len(), 2 + LINKS + LINKS); // the photo, the depth, each region, each step
    Ok(())
}

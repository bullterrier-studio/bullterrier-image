//! A recipe whose regions refer to one another along a chain thousands long, or by thousands of paths to one region, is
//! read, checked, rendered, traced, written out and dropped on a thread of 128 KB: the walks over regions keep their
//! place in a list, not in the call stack. (Values nested inside a step are read by `serde_json` and `toml`, which recurse:
//! `tests/_native/test__core.py` reads the deepest on a small stack, in the build that ships.)

use std::error::Error;
use std::thread;

use bullterrier_image::{Layer, Options, Photo, Picture, Presets, Recipe};
use serde_json::json;

/// Links in the chain of bands, and in the chain of combines each of which refers to the one before twice.
const LINKS: usize = 2_000;

#[test]
fn a_long_chain_and_a_wide_diamond_of_regions_need_no_deep_call_stack() -> Result<(), Box<dyn Error + Send + Sync>> {
    thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(|| {
            let mut regions = serde_json::Map::new();
            for link in 0..LINKS {
                let (band_of, combine_of) = if link == 0 {
                    ("depth".to_owned(), "depth".to_owned())
                } else {
                    (format!("band{}", link - 1), format!("combine{}", link - 1))
                };
                regions.insert(
                    format!("band{link}"),
                    json!({"kind": "band", "of": band_of, "low": 0.0, "high": 1.0, "feather": 0.0}),
                );
                regions.insert(
                    format!("combine{link}"),
                    json!({"kind": "combine", "op": "max", "of": [combine_of.clone(), combine_of]}),
                );
            }
            let last = LINKS - 1;
            let steps = json!([
                {"effect": "grade", "params": {"exposure": 0.1}, "mask": format!("band{last}")},
                {"effect": "grade", "params": {"exposure": 0.1}, "mask": format!("combine{last}")},
            ]);
            let recipe = Recipe::from_json(&json!({"steps": steps, "regions": regions}).to_string())?;
            recipe.clone().checked(&Presets::new())?;
            let rgb = vec![0.5_f32; 4 * 6 * 3];
            let photo = Photo::new(Picture::new(4, 6, &rgb)?).with_map("depth", Layer::new(4, 6, vec![0.5; 24])?)?;
            let rendered = photo.render(&recipe, &Presets::new(), Options::default())?;
            assert_eq!((rendered.height(), rendered.width()), (4, 6));
            assert_eq!(photo.trace(&recipe, &Presets::new(), Options::default())?.len(), 4);
            assert!(serde_json::to_string(&recipe)?.contains(&format!("band{last}")));
            Ok(())
        })?
        .join()
        .map_err(|_| "the thread panicked")?
}

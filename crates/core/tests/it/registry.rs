//! The registry, through the public API alone: the core's effects are found by name with what each declares, a
//! registered effect joins them and leaves again, and a name is held by one effect at a time.

use std::sync::Arc;

use crate::common::Outcome;
use crate::common::run::{Paint, unique};
use bullterrier_image::{
    Category, Effect, Error, Image, Input, Inputs, Picture, effect, effects, register, unregister,
};
use serde_json::{Value, json};

fn paint(name: &str) -> Arc<dyn Effect> {
    Arc::new(Paint::named(name))
}

#[test]
fn the_core_effects_are_found_by_name_with_what_each_declares() -> Outcome {
    // (name, category, opacity, inputs): what each effect file declares in its META
    for (name, category) in [
        ("grade", Category::Colour),
        ("pattern", Category::Backgrounds),
        ("pixel_grid", Category::Texture),
    ] {
        let effect = effect(name)?;
        assert_eq!(effect.name(), name);
        assert_eq!(effect.category(), category, "{name}");
        assert!((effect.opacity() - 1.0).abs() < f64::EPSILON, "{name}");
        assert!(effect.inputs().is_empty(), "{name}");
        assert!(effect.params().is_some_and(|table| !table.fields.is_empty()), "{name}");
    }
    Ok(())
}

#[test]
fn an_unknown_name_is_refused_by_name() {
    assert!(matches!(effect("nope"), Err(Error::UnknownEffect(name)) if name == "nope"));
}

#[test]
fn the_names_are_sorted_and_a_registered_effect_is_among_them() -> Outcome {
    register(paint("aaa_first"))?;
    let names = effects();
    unregister("aaa_first");
    assert_eq!(names.first().map(String::as_str), Some("aaa_first"));
    assert!(names.contains(&"grade".to_owned()));
    assert!(names.is_sorted(), "{names:?}");
    Ok(())
}

#[test]
fn a_registered_effect_runs_by_its_name_and_declares_what_it_says() -> Outcome {
    let name = unique("painter");
    register(paint(&name))?;
    let found = effect(&name);
    unregister(&name);
    let found = found?;
    assert_eq!(
        (found.category(), found.inputs()),
        (Category::Utilities, &[Input::SizeRatio][..])
    );
    assert!((found.opacity() - 0.25).abs() < f64::EPSILON);
    assert!(found.params().is_none());
    let values = [0.0_f32; 12];
    let image = found.apply(&Picture::new(2, 2, &values)?, json!({"level": 0.5}), &Inputs::default())?;
    assert_eq!(image.into_values(), vec![0.5_f32; 12]);
    Ok(())
}

#[test]
fn a_failure_of_a_registered_effect_comes_back_naming_it() -> Outcome {
    let name = unique("painter");
    register(paint(&name))?;
    let found = effect(&name);
    unregister(&name);
    let values = [0.0_f32; 3];
    let result = found?.apply(&Picture::new(1, 1, &values)?, json!({}), &Inputs::default());
    assert!(matches!(result, Err(Error::Foreign { effect, .. }) if effect == name));
    Ok(())
}

#[test]
fn a_name_taken_by_a_core_or_a_registered_effect_is_refused() -> Outcome {
    let painter = unique("painter");
    register(paint(&painter))?;
    for taken in ["grade", painter.as_str()] {
        let result = register(paint(taken));
        assert!(
            matches!(&result, Err(Error::Duplicate(name)) if name == taken),
            "{taken}: {result:?}"
        );
    }
    unregister(&painter);
    Ok(())
}

#[test]
fn an_unregistered_effect_is_gone_and_its_name_free_again_but_the_core_effects_stay() -> Outcome {
    let name = unique("painter");
    register(paint(&name))?;
    assert!(unregister(&name));
    assert!(!unregister(&name));
    assert!(matches!(effect(&name), Err(Error::UnknownEffect(_))));
    register(paint(&name))?;
    unregister(&name);
    assert!(!unregister("grade"));
    assert!(effect("grade").is_ok());
    Ok(())
}

/// An effect that asks the registry for the names when it is dropped, as a Python callable's `__del__` may.
struct AsksOnDrop {
    name: String,
    asked: std::sync::mpsc::Sender<usize>,
}

impl Effect for AsksOnDrop {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn apply(&self, picture: &Picture<'_>, _params: Value, _inputs: &Inputs<'_>) -> Result<Image, Error> {
        Ok(picture.map(|pixel| pixel))
    }
}

impl Drop for AsksOnDrop {
    fn drop(&mut self) {
        let _ = self.asked.send(effects().len());
    }
}

#[test]
fn an_unregistered_effect_that_uses_the_registry_as_it_is_dropped_does_not_deadlock() -> Outcome {
    let (asked, answer) = std::sync::mpsc::channel();
    let name = unique("asks_on_drop");
    register(Arc::new(AsksOnDrop {
        name: name.clone(),
        asked,
    }))?;
    // On its own thread, so a deadlock fails the test instead of hanging it.
    std::thread::spawn(move || unregister(&name));
    let names = answer.recv_timeout(std::time::Duration::from_secs(5));
    assert!(
        names.is_ok(),
        "dropping the unregistered effect deadlocked on the registry"
    );
    Ok(())
}

//! The registry: every effect the core can run by name, its own and the ones registered at run time.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, PoisonError, RwLock};

use crate::Error;
use crate::effect::Effect;

type Table = BTreeMap<String, Arc<dyn Effect>>;

/// A set of effects by name: the core's own, fixed, and the registered ones, which come and go.
pub(crate) struct Registry {
    builtin: Table,
    registered: RwLock<Table>,
}

impl Registry {
    /// A registry of the core's own effects.
    pub(crate) fn new() -> Self {
        let builtin = crate::effects::builtin()
            .into_iter()
            .map(|effect| (effect.name().to_owned(), effect))
            .collect();
        Self {
            builtin,
            registered: RwLock::new(Table::new()),
        }
    }

    /// Add `effect` under its name; [`Error::Duplicate`] if the name is taken.
    pub(crate) fn register(&self, effect: Arc<dyn Effect>) -> Result<(), Error> {
        let name = effect.name().to_owned();
        let mut registered = self.registered.write().unwrap_or_else(PoisonError::into_inner);
        if self.builtin.contains_key(&name) || registered.contains_key(&name) {
            return Err(Error::Duplicate(name));
        }
        registered.insert(name, effect);
        Ok(())
    }

    /// Remove the registered effect called `name`; false if there is none (the core's own cannot be removed).
    pub(crate) fn unregister(&self, name: &str) -> bool {
        let removed = self
            .registered
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(name);
        // The lock is released: dropping a foreign effect may run code (a Python `__del__`) that uses the registry.
        removed.is_some()
    }

    /// The effect called `name`; [`Error::UnknownEffect`] if there is none.
    pub(crate) fn effect(&self, name: &str) -> Result<Arc<dyn Effect>, Error> {
        let registered = self.registered.read().unwrap_or_else(PoisonError::into_inner);
        self.builtin
            .get(name)
            .or_else(|| registered.get(name))
            .cloned()
            .ok_or_else(|| Error::UnknownEffect(name.to_owned()))
    }

    /// Every name, sorted.
    pub(crate) fn names(&self) -> Vec<String> {
        let registered = self.registered.read().unwrap_or_else(PoisonError::into_inner);
        let mut names: Vec<String> = self.builtin.keys().chain(registered.keys()).cloned().collect();
        names.sort();
        names
    }
}

static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::new);

/// Add `effect` to the registry under its name, beside the core's own: it then runs by that name like them.
///
/// # Errors
/// [`Error::Duplicate`] if an effect of that name is there already, the core's own or another registered one.
pub fn register(effect: Arc<dyn Effect>) -> Result<(), Error> {
    REGISTRY.register(effect)
}

/// Remove the effect registered as `name`; whether there was one. The core's own effects stay.
pub fn unregister(name: &str) -> bool {
    REGISTRY.unregister(name)
}

/// The effect called `name`.
///
/// # Errors
/// [`Error::UnknownEffect`] if there is none.
pub fn effect(name: &str) -> Result<Arc<dyn Effect>, Error> {
    REGISTRY.effect(name)
}

/// The names of the effects, the core's own and the registered ones, sorted.
#[must_use]
pub fn effects() -> Vec<String> {
    REGISTRY.names()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::{Category, Input};
    use crate::{Image, Picture};
    use serde_json::{Value, json};

    /// A foreign effect that paints the picture with the first number of its parameters; counts nothing, mocks nothing.
    struct Paint {
        name: &'static str,
    }

    impl Effect for Paint {
        fn name(&self) -> &str {
            self.name
        }
        fn category(&self) -> Category {
            Category::Utilities
        }
        fn opacity(&self) -> f64 {
            0.25
        }
        fn inputs(&self) -> &[Input] {
            &[Input::SizeRatio]
        }
        fn apply(&self, picture: &Picture<'_>, params: Value) -> Result<Image, Error> {
            let level = params["level"].as_f64().ok_or_else(|| Error::Foreign {
                effect: self.name.to_owned(),
                message: "no level".to_owned(),
            })?;
            #[expect(clippy::cast_possible_truncation, reason = "a test level is a small number")]
            Ok(picture.map(|_| [level as f32; 3]))
        }
    }

    fn paint(name: &'static str) -> Arc<dyn Effect> {
        Arc::new(Paint { name })
    }

    #[test]
    fn the_core_effects_are_found_by_name_with_what_each_declares() -> Result<(), Error> {
        let registry = Registry::new();
        // (name, category, opacity, inputs): what each effect file declares in its META
        for (name, category) in [
            ("grade", Category::Colour),
            ("pattern", Category::Backgrounds),
            ("pixel_grid", Category::Texture),
        ] {
            let effect = registry.effect(name)?;
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
        assert!(matches!(Registry::new().effect("nope"), Err(Error::UnknownEffect(name)) if name == "nope"));
    }

    #[test]
    fn the_names_are_sorted_and_a_registered_effect_is_among_them() -> Result<(), Error> {
        let registry = Registry::new();
        registry.register(paint("aaa_first"))?;
        let names = registry.names();
        assert_eq!(names.first().map(String::as_str), Some("aaa_first"));
        assert!(names.contains(&"grade".to_owned()));
        assert!(names.is_sorted(), "{names:?}");
        Ok(())
    }

    #[test]
    fn a_registered_effect_runs_by_its_name_and_declares_what_it_says() -> Result<(), Error> {
        let registry = Registry::new();
        registry.register(paint("painter"))?;
        let found = registry.effect("painter")?;
        assert_eq!(
            (found.category(), found.inputs()),
            (Category::Utilities, &[Input::SizeRatio][..])
        );
        assert!((found.opacity() - 0.25).abs() < f64::EPSILON);
        assert!(found.params().is_none());
        let values = [0.0_f32; 12];
        let image = found.apply(&Picture::new(2, 2, &values)?, json!({"level": 0.5}))?;
        assert_eq!(image.into_values(), vec![0.5_f32; 12]);
        Ok(())
    }

    #[test]
    fn a_failure_of_a_registered_effect_comes_back_naming_it() -> Result<(), Error> {
        let registry = Registry::new();
        registry.register(paint("painter"))?;
        let values = [0.0_f32; 3];
        let result = registry
            .effect("painter")?
            .apply(&Picture::new(1, 1, &values)?, json!({}));
        assert!(matches!(result, Err(Error::Foreign { effect, .. }) if effect == "painter"));
        Ok(())
    }

    #[test]
    fn a_name_taken_by_a_core_or_a_registered_effect_is_refused() -> Result<(), Error> {
        let registry = Registry::new();
        registry.register(paint("painter"))?;
        for taken in ["grade", "painter"] {
            let result = registry.register(paint(taken));
            assert!(
                matches!(&result, Err(Error::Duplicate(name)) if name == taken),
                "{taken}: {result:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn an_unregistered_effect_is_gone_and_its_name_free_again_but_the_core_effects_stay() -> Result<(), Error> {
        let registry = Registry::new();
        registry.register(paint("painter"))?;
        assert!(registry.unregister("painter"));
        assert!(!registry.unregister("painter"));
        assert!(matches!(registry.effect("painter"), Err(Error::UnknownEffect(_))));
        registry.register(paint("painter"))?;
        assert!(!registry.unregister("grade"));
        assert!(registry.effect("grade").is_ok());
        Ok(())
    }

    /// An effect that asks its registry for the names when it is dropped, as a Python callable's `__del__` may.
    struct AsksOnDrop {
        registry: Arc<Registry>,
        asked: std::sync::mpsc::Sender<usize>,
    }

    impl Effect for AsksOnDrop {
        fn name(&self) -> &'static str {
            "asks_on_drop"
        }
        fn category(&self) -> Category {
            Category::Utilities
        }
        fn opacity(&self) -> f64 {
            1.0
        }
        fn inputs(&self) -> &[Input] {
            &[]
        }
        fn apply(&self, picture: &Picture<'_>, _params: Value) -> Result<Image, Error> {
            Ok(picture.map(|pixel| pixel))
        }
    }

    impl Drop for AsksOnDrop {
        fn drop(&mut self) {
            let _ = self.asked.send(self.registry.names().len());
        }
    }

    #[test]
    fn an_unregistered_effect_that_uses_the_registry_as_it_is_dropped_does_not_deadlock() -> Result<(), Error> {
        let (asked, answer) = std::sync::mpsc::channel();
        let registry = Arc::new(Registry::new());
        registry.register(Arc::new(AsksOnDrop {
            registry: Arc::clone(&registry),
            asked,
        }))?;
        // On its own thread, so a deadlock fails the test instead of hanging it.
        let unregistering = Arc::clone(&registry);
        std::thread::spawn(move || unregistering.unregister("asks_on_drop"));
        let names = answer.recv_timeout(std::time::Duration::from_secs(5));
        assert!(
            names.is_ok(),
            "dropping the unregistered effect deadlocked on the registry"
        );
        Ok(())
    }
}

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

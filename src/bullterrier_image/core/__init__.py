"""The kernel: the effect registry, the contract types every effect is written against, and the recipe engine.

    registry    effects register here (`@effect`); everything else looks them up here
    params      `EffectParams`, `param`, `Color`: how an effect declares its validated, documented parameters
    types       the forms a colour may be written in
    vocabulary  parameter names effects must not use, and what to use instead
    steps       `Step` and `Recipe`: the data model of a look and its validation
    runner      running a recipe on a photo; its maps are given to it (by name, as arrays or as `Finder`s)

`core` imports only `ops` from the rest of bullterrier_image (import-linter enforces it), so it stays small and stable
while effects, adapters and the app change around it. Its names are imported from their modules
(`from bullterrier_image.core.registry import effect`); the package itself re-exports nothing, so each name has one
home. Users import from `bullterrier_image` / `bullterrier_image.recipe`, whose `render` & co. also know how to find the
subject.
"""

from . import params, registry, runner, steps, types, vocabulary

__all__ = ["params", "registry", "runner", "steps", "types", "vocabulary"]

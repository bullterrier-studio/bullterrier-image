"""The kernel: the effect registry, the contract types every effect is written against, and the recipe engine.

    errors      `PhotoFxError` and its subclasses: the exceptions the library raises on purpose
    registry    effects register here (`@effect`); everything else looks them up here
    params      `EffectParams`, `param`, `Color`: how an effect declares its validated, documented parameters
    types       the forms a colour may be written in
    vocabulary  parameter names effects must not use, and what to use instead
    timing      values changing over a clip: easing, `Animated`, `ramp`, `Animation`
    steps       `Step` and `Recipe`: the data model of a look and its validation
    loader      `load_recipe`: the TOML format
    runner      running a recipe on a photo; the subject is given to it (a matte or a `find_subject` function)

`core` imports only `ops` from the rest of photo_fx (import-linter enforces it), so it stays small and stable while
effects, adapters and the app change around it. Its names are imported from their modules
(`from photo_fx.core.registry import effect`); the package itself re-exports nothing, so each name has one home.
Users import from `photo_fx` / `photo_fx.recipe`, whose `render` & co. also know how to find the subject.
"""

from . import errors, loader, params, registry, runner, steps, timing, types, vocabulary

__all__ = ["errors", "loader", "params", "registry", "runner", "steps", "timing", "types", "vocabulary"]

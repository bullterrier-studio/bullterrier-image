"""The kernel: the effect registry, the contract types every effect is written against, and the recipe engine.

    registry    effects register here (`@effect`); everything else looks them up here
    params      `EffectParams`, `param`, `Color`: how an effect declares its validated, documented parameters
    types       the forms a colour may be written in
    vocabulary  parameter names effects must not use, and what to use instead
    timing      values changing over a clip: easing, `Animated`, `ramp`, `Animation`
    steps       `Step` and `Recipe`: the data model of a look and its validation
    loader      `load_recipe`: the TOML format
    runner      running a recipe on a photo; the subject is given to it (a matte or a `find_subject` function)

`core` imports only `ops` from the rest of photo_fx (import-linter enforces it), so it stays small and stable while
effects, adapters and the app change around it.
Users import from `photo_fx` / `photo_fx.recipe`, whose `render` & co. also know how to find the subject.
"""

from .loader import load_recipe
from .params import UNITS, Color, EffectParams, param
from .registry import (
    CATEGORIES,
    Effect,
    all_effects,
    apply,
    categories,
    category,
    describe,
    effect,
    get,
    summary,
    unregister,
)
from .runner import Stage, SubjectFinder, render, render_all, render_frames, trace
from .steps import Recipe, Step
from .timing import Animated, Animation, ramp
from .vocabulary import DISCOURAGED

__all__ = [
    "CATEGORIES", "DISCOURAGED", "UNITS", "Animated", "Animation", "Color", "Effect", "EffectParams", "Recipe", "Stage",
    "Step", "SubjectFinder", "all_effects", "apply", "categories", "category", "describe", "effect", "get",
    "load_recipe", "param", "ramp", "render", "render_all", "render_frames", "summary", "trace", "unregister",
]  # fmt: skip

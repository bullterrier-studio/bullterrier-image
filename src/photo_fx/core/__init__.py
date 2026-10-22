"""The kernel: the effect registry, the contract types every effect is written against, and the recipe engine.

    registry    effects register here (`@effect`); everything else looks them up here
    types       `Color`, the colour forms an effect parameter accepts
    vocabulary  parameter names effects must not use, and what to use instead
    timing      values changing over a clip: easing, `Animated`, `ramp`, `Animation`
    steps       `Step` and `Recipe`: the data model of a look and its validation
    loader      `load_recipe`: the TOML format
    runner      running a recipe on a photo; the subject is given to it (a matte or a `find_subject` function)

`core` imports nothing from photo_fx outside itself (import-linter enforces it; the one recorded exception is
`runner` using `ops.resample`), so it stays small and stable while effects, adapters and the app change around it.
Users import from `photo_fx` / `photo_fx.recipe`, whose `render` & co. also know how to find the subject.
"""

from .loader import load_recipe
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
from .types import Color
from .vocabulary import DISCOURAGED

__all__ = [
    "CATEGORIES", "DISCOURAGED", "Animated", "Animation", "Color", "Effect", "Recipe", "Stage", "Step", "SubjectFinder",
    "all_effects", "apply", "categories", "category", "describe", "effect", "get", "load_recipe", "ramp", "render",
    "render_all", "render_frames", "summary", "trace", "unregister",
]  # fmt: skip

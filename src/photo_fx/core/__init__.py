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
effects, adapters and the app change around it.
Users import from `photo_fx` / `photo_fx.recipe`, whose `render` & co. also know how to find the subject.
"""

from .errors import (
    BackendUnavailableError,
    ImageReadError,
    PhotoFxError,
    RecipeError,
    SubjectNotFoundError,
    UnknownEffectError,
)
from .loader import load_recipe
from .params import UNITS, Color, EffectParams, param
from .registry import (
    CATEGORIES,
    INPUTS,
    Effect,
    ModelledEffect,
    all_effects,
    apply,
    categories,
    category,
    describe,
    documentation,
    effect,
    get,
    inputs,
    parameters,
    schema,
    summary,
    unregister,
)
from .runner import Stage, SubjectFinder, render, render_all, render_frames, trace
from .steps import Recipe, Step
from .timing import Animated, Animation, ramp
from .vocabulary import DISCOURAGED

__all__ = [
    "CATEGORIES", "DISCOURAGED", "INPUTS", "UNITS", "Animated", "Animation", "BackendUnavailableError", "Color",
    "Effect", "EffectParams", "ImageReadError", "ModelledEffect", "PhotoFxError", "Recipe", "RecipeError", "Stage",
    "Step", "SubjectFinder", "SubjectNotFoundError", "UnknownEffectError", "all_effects", "apply", "categories",
    "category",
    "describe", "documentation", "effect", "get", "inputs", "load_recipe", "param", "parameters", "ramp", "render",
    "render_all", "render_frames", "schema", "summary", "trace", "unregister",
]  # fmt: skip

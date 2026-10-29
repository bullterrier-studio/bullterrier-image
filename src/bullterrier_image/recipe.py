"""Recipes: the one public place to import everything about them from.

A recipe is an ordered list of effects (written in Python or in a small TOML file) that is run on a photo, and the
regions (`Grid`, `Shards`, `Cuts`, `Path`, `Svg`, `Pick`, `Subject`, `Band`, `Bands`, `Combine`, `LinearGradient`,
`RadialGradient`, `Given`) its steps name: `mask` limits a step to one, `Each` gives each piece of one its own look. The
machinery lives in `bullterrier_image.core` (`steps`, `runner`) and `bullterrier_image.loader`; this module re-exports
its public names and binds what the core leaves open: how the subject and the depth are found. `render`, `trace` and
`render_all` take the same arguments as the core's, but when a recipe will ask for the subject (it needs the subject and
no `subject` map is given, or it finds the subject piece by piece) and `finders` has no `subject`, they find it with
`bullterrier_image.finding.subject.subject_finder()`; when a recipe needs depth and neither a `depth` map nor a `depth`
finder is given, with `bullterrier_image.finding.depth.depth_finder()`. Each is built only then, so a recipe that needs
neither never depends on the model settings. Any other map (a `Given` region's) is handed in by name in `maps`.
"""

from collections.abc import Mapping, Sequence

import numpy as np

from .core import looks as shipped_looks
from .core import runner
from .core.framing import Framing
from .core.params import Drive
from .core.regions import (
    FOUND_MAPS,
    Band,
    Bands,
    Combine,
    Curve,
    Cuts,
    Edge,
    Given,
    Grid,
    Line,
    LinearGradient,
    Path,
    Pick,
    RadialGradient,
    Shards,
    Subject,
    Svg,
)
from .core.runner import Finder, Stage
from .core.steps import Each, Parameter, Recipe, Step, maps_needed, needs_finder
from .errors import RecipeError
from .finding import subject
from .finding.depth import depth_finder
from .loader import load_recipe

__all__ = [
    "Band",
    "Bands",
    "Combine",
    "Curve",
    "Cuts",
    "Drive",
    "Each",
    "Edge",
    "Finder",
    "Framing",
    "Given",
    "Grid",
    "Line",
    "LinearGradient",
    "Parameter",
    "Path",
    "Pick",
    "RadialGradient",
    "Recipe",
    "Shards",
    "Stage",
    "Step",
    "Subject",
    "Svg",
    "load_recipe",
    "looks",
    "render",
    "render_all",
    "trace",
]


def looks() -> dict[str, str]:
    """The looks that ship with bullterrier-image, by name, each with what it looks like in a sentence; load one with
    `load_recipe(name)`."""
    return {name: look.description for name, look in shipped_looks.read().items()}


def render(
    rgb: np.ndarray,
    recipe: Recipe | Step,
    *,
    maps: Mapping[str, np.ndarray] | None = None,
    finders: Mapping[str, Finder] | None = None,
    size: int | None = None,
) -> np.ndarray:
    """One still of `recipe` on `rgb`: `bullterrier_image.core.runner.render` with the default finders. `size` is the
    longest side of the result in px: the recipe's pixel values (written for 480) are scaled to it."""
    recipe = _one_recipe(recipe)
    return runner.render(rgb, recipe, maps=maps, finders=_finders([recipe], maps, finders), size=size)


def trace(
    rgb: np.ndarray,
    recipe: Recipe | Step,
    *,
    maps: Mapping[str, np.ndarray] | None = None,
    finders: Mapping[str, Finder] | None = None,
    size: int | None = None,
) -> list[Stage]:
    """Every stage of a render: `bullterrier_image.core.runner.trace` with the default finders; `size` as for
    `render`."""
    recipe = _one_recipe(recipe)
    return runner.trace(rgb, recipe, maps=maps, finders=_finders([recipe], maps, finders), size=size)


def render_all(
    rgb: np.ndarray,
    recipes: Sequence[Recipe],
    *,
    maps: Mapping[str, np.ndarray] | None = None,
    finders: Mapping[str, Finder] | None = None,
    size: int | None = None,
) -> list[np.ndarray]:
    """Several recipes on one photo: `bullterrier_image.core.runner.render_all` with the default finders. `size` as for
    `render`."""
    recipes = _recipes(recipes)
    return runner.render_all(rgb, recipes, maps=maps, finders=_finders(recipes, maps, finders), size=size)


def _one_recipe(recipe: object) -> Recipe:
    """`recipe` as the `Recipe` to run: itself, or a `Step` as the recipe of that one step. RecipeError, naming the fix,
    for anything else: the one check of what `render` and `trace` are given as the recipe."""
    if isinstance(recipe, Recipe):
        return recipe
    if isinstance(recipe, Step):
        return Recipe([recipe])
    fix = {
        np.ndarray: "the photo comes first, then the recipe: render(photo, recipe)",
        str: 'a look is loaded first: load_recipe("glitch"), or a file with load_recipe(path)',
    }.get(type(recipe), "wrap steps in a recipe: Recipe([step, ...])")
    raise RecipeError(f"the recipe is a Recipe or a Step, got {type(recipe).__name__}: {fix}")


def _recipes(recipes: object) -> list[Recipe]:
    """`recipes` as the list of `Recipe` to run; RecipeError, naming the fix, unless it is a list or tuple of them:
    the one check of what `render_all` is given."""
    if not isinstance(recipes, list | tuple) or not all(isinstance(recipe, Recipe) for recipe in recipes):
        found = type(recipes).__name__ + (
            f" of {', '.join(sorted({type(item).__name__ for item in recipes}))}"
            if isinstance(recipes, list | tuple)
            else ""
        )
        raise RecipeError(
            f"the recipes are a sequence of Recipe (a list or tuple), got {found}: one recipe is "
            "render(photo, recipe); steps are wrapped in Recipe([step, ...])"
        )
    return list(recipes)


def _finders(
    recipes: list[Recipe], maps: Mapping[str, np.ndarray] | None, finders: Mapping[str, Finder] | None
) -> dict[str, Finder]:
    """`finders` with the default finder of the subject and of the depth added when none is named and one of `recipes`
    will ask for it: the subject when a recipe needs it and no `subject` map is given, or finds it piece by piece; the
    depth when a recipe needs it and no `depth` map is given. Each default is built only then, once per call."""
    maps, bound = maps or {}, dict(finders or {})
    for name in FOUND_MAPS:
        asks = name not in maps and any(name in maps_needed(recipe) for recipe in recipes)
        asks = asks or (name == "subject" and any(needs_finder(recipe) for recipe in recipes))
        if name not in bound and asks:
            bound[name] = default_finder(name)
    return bound


def default_finder(name: str) -> Finder:
    """The default finder of the built-in map `name` (`subject` or `depth`): `finding.subject.subject_finder()`, or
    `finding.depth.depth_finder()`. Built when called, so a setting nobody uses cannot stop a run."""
    return {"subject": subject.subject_finder, "depth": depth_finder}[name]()


# Each function above is the core's, with the default finders bound (and, for one picture, a `Step` taken as the recipe
# of that one step): its one-line docstring is replaced by the core's full documentation, plus that.
_ONE_STEP = (
    " A `Step` given as the recipe is the recipe of that one step: `render(photo, step)` is `step.apply(photo)`."
)
for _public in (render, trace, render_all):
    _public.__doc__ = (
        getattr(runner, _public.__name__).__doc__.rstrip()
        + "\n\n    Without a `subject` finder, the subject is found by `finding.subject.subject_finder()` if needed;"
        + " without a `depth` map or finder, the depth by `finding.depth.depth_finder()`."
        + ("" if _public is render_all else _ONE_STEP)
        + "\n    "
    )
del _public

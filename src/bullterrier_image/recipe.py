"""Recipes: the one public place to import everything about them from.

A recipe is an ordered list of effects (written in Python or in a small TOML file) that is run on a photo, and the
regions (`Grid`, `Shards`, `Cuts`, `Path`, `Svg`, `Pick`, `Subject`, `Depth`, `DepthBands`) its steps name: `mask`
limits a step to one, `Each` gives each piece of one its own look. The machinery lives in `bullterrier_image.core`
(`steps`, `runner`) and `bullterrier_image.loader`; this module re-exports its public names and binds what the core
leaves open: how the subject and the depth are found. `render`, `trace` and `render_all` take the same arguments as the
core's, but when a recipe will ask for the subject (it needs the subject and no `matte` is given, or it finds the
subject piece by piece) and no `find_subject` is given, they find it with
`bullterrier_image.finding.subject.subject_finder()`; when a recipe needs depth and neither `depth` nor `find_depth` is
given, with `bullterrier_image.finding.depth.depth_finder()`. Each is built only then, so a recipe that needs neither
never depends on the model settings.
"""

import numpy as np

from .core import runner
from .core.framing import Framing
from .core.regions import Curve, Cuts, Depth, DepthBands, Edge, Grid, Line, Path, Pick, Shards, Subject, Svg
from .core.runner import Finder, Stage
from .core.steps import Each, Recipe, Step
from .finding import subject
from .finding.depth import depth_finder
from .loader import load_recipe

__all__ = [
    "Curve",
    "Cuts",
    "Depth",
    "DepthBands",
    "Each",
    "Edge",
    "Finder",
    "Framing",
    "Grid",
    "Line",
    "Path",
    "Pick",
    "Recipe",
    "Shards",
    "Stage",
    "Step",
    "Subject",
    "Svg",
    "load_recipe",
    "render",
    "render_all",
    "trace",
]


def render(
    rgb: np.ndarray,
    recipe: Recipe,
    *,
    matte: np.ndarray | None = None,
    depth: np.ndarray | None = None,
    find_subject: Finder | None = None,
    find_depth: Finder | None = None,
) -> np.ndarray:
    """One still of `recipe` on `rgb`: `bullterrier_image.core.runner.render` with the default finders."""
    find_subject, find_depth = _finders([recipe], matte, depth, find_subject, find_depth)
    return runner.render(rgb, recipe, matte=matte, depth=depth, find_subject=find_subject, find_depth=find_depth)


def trace(
    rgb: np.ndarray,
    recipe: Recipe,
    *,
    matte: np.ndarray | None = None,
    depth: np.ndarray | None = None,
    find_subject: Finder | None = None,
    find_depth: Finder | None = None,
) -> list[Stage]:
    """Every stage of a render: `bullterrier_image.core.runner.trace` with the default finders."""
    find_subject, find_depth = _finders([recipe], matte, depth, find_subject, find_depth)
    return runner.trace(rgb, recipe, matte=matte, depth=depth, find_subject=find_subject, find_depth=find_depth)


def render_all(
    rgb: np.ndarray,
    recipes: list[Recipe],
    *,
    matte: np.ndarray | None = None,
    depth: np.ndarray | None = None,
    find_subject: Finder | None = None,
    find_depth: Finder | None = None,
) -> list[np.ndarray]:
    """Several recipes on one photo: `bullterrier_image.core.runner.render_all` with the default finders."""
    find_subject, find_depth = _finders(recipes, matte, depth, find_subject, find_depth)
    return runner.render_all(rgb, recipes, matte=matte, depth=depth, find_subject=find_subject, find_depth=find_depth)


def _finders(
    recipes: list[Recipe],
    matte: np.ndarray | None,
    depth: np.ndarray | None,
    find_subject: Finder | None,
    find_depth: Finder | None,
) -> tuple[Finder | None, Finder | None]:
    """(`find_subject`, `find_depth`), each replaced by its default finder when it is None and one of `recipes` will
    ask for it: the subject when a recipe needs it and no `matte` is given, or finds it piece by piece; the depth when a
    recipe needs it and no `depth` is given. Each default is built only then, once per call."""
    asks_subject = any((recipe.needs_matte and matte is None) or recipe.needs_finder for recipe in recipes)
    asks_depth = depth is None and any(recipe.needs_depth for recipe in recipes)
    return (
        subject.subject_finder() if find_subject is None and asks_subject else find_subject,
        depth_finder() if find_depth is None and asks_depth else find_depth,
    )


# Each function above is the core's, with the default finders bound: its one-line docstring is replaced by the core's
# full documentation, plus that.
for _public in (render, trace, render_all):
    _public.__doc__ = (
        getattr(runner, _public.__name__).__doc__.rstrip()
        + "\n\n    Without `find_subject`, the subject is found by `finding.subject.subject_finder()` if needed;"
        + " without `depth` and `find_depth`, the depth by `finding.depth.depth_finder()`.\n    "
    )
del _public

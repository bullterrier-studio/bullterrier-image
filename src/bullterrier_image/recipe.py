"""Recipes: the one public place to import everything about them from.

A recipe is an ordered list of effects (written in Python or in a small TOML file) that is run on a photo, and the
regions (`Grid`, `Shards`, `Cuts`, `Path`, `Svg`, `Pick`, `Subject`) its steps name: `mask` limits a step to one,
`Each` gives each piece of one its own look. The machinery lives in `bullterrier_image.core` (`steps`, `runner`) and
`bullterrier_image.loader`; this module re-exports its public names and binds the one thing the core leaves open: how
the subject is found. `render`, `trace` and `render_all` take the same arguments as the core's, but when a recipe will
ask for the subject (it needs the subject and no `matte` is given, or it finds the subject piece by piece) and no
`find_subject` is given, they find it with `bullterrier_image.finding.subject.subject_finder()`, built only then (so a
recipe that needs no subject never depends on the segmenter settings).
"""

import numpy as np

from .core import runner
from .core.framing import Framing
from .core.regions import Curve, Cuts, Edge, Grid, Line, Path, Pick, Shards, Subject, Svg
from .core.runner import Finder, Stage
from .core.steps import Each, Recipe, Step
from .finding import subject
from .loader import load_recipe

__all__ = [
    "Curve",
    "Cuts",
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
    find_subject: Finder | None = None,
) -> np.ndarray:
    """One still of `recipe` on `rgb`: `bullterrier_image.core.runner.render` with the default subject finder."""
    return runner.render(rgb, recipe, matte=matte, find_subject=_finder(find_subject, matte, [recipe]))


def trace(
    rgb: np.ndarray,
    recipe: Recipe,
    *,
    matte: np.ndarray | None = None,
    find_subject: Finder | None = None,
) -> list[Stage]:
    """Every stage of a render: `bullterrier_image.core.runner.trace` with the default subject finder."""
    return runner.trace(rgb, recipe, matte=matte, find_subject=_finder(find_subject, matte, [recipe]))


def render_all(
    rgb: np.ndarray,
    recipes: list[Recipe],
    *,
    matte: np.ndarray | None = None,
    find_subject: Finder | None = None,
) -> list[np.ndarray]:
    """Several recipes on one photo: `bullterrier_image.core.runner.render_all` with the default subject finder."""
    return runner.render_all(rgb, recipes, matte=matte, find_subject=_finder(find_subject, matte, recipes))


def _finder(find_subject: Finder | None, matte: np.ndarray | None, recipes: list[Recipe]) -> Finder | None:
    """`find_subject`, else the default finder when one of `recipes` will ask for the subject: it needs the subject and
    no `matte` is given, or it finds the subject piece by piece. Built only then, once per call."""
    asks = any((recipe.needs_matte and matte is None) or recipe.needs_finder for recipe in recipes)
    return subject.subject_finder() if find_subject is None and asks else find_subject


# Each function above is the core's, with the default finder bound: its one-line docstring is replaced by the core's
# full documentation, plus that.
for _public in (render, trace, render_all):
    _public.__doc__ = (
        getattr(runner, _public.__name__).__doc__.rstrip()
        + "\n\n    Without `find_subject`, the subject is found by `finding.subject.subject_finder()` if needed.\n    "
    )
del _public

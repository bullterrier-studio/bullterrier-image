"""Recipes: the one public place to import everything about them from.

A recipe is an ordered list of effects (written in Python or in a small TOML file) that is run on a photo and can be
animated. The machinery lives in `photo_fx.core` (`timing`, `steps`, `loader`, `runner`); this module re-exports its
public names and binds the one thing the core leaves open: how the subject is found. `render`, `render_frames`,
`trace` and `render_all` take the same arguments as the core's, but when a masked step needs the subject and no
`matte` or `find_subject` is given, they find it with `photo_fx.matte.subject_matte`.
"""

from collections.abc import Iterator

import numpy as np

from . import matte as subject
from .core import runner
from .core.loader import load_recipe
from .core.runner import Stage, SubjectFinder
from .core.steps import Recipe, Step
from .core.timing import Animated, Animation, ramp

__all__ = [
    "Animated", "Animation", "Recipe", "Stage", "Step", "SubjectFinder", "load_recipe", "ramp", "render", "render_all",
    "render_frames", "trace",
]  # fmt: skip


def render(
    rgb: np.ndarray,
    recipe: Recipe,
    matte: np.ndarray | None = None,
    at: float = 0.0,
    *,
    find_subject: SubjectFinder | None = None,
) -> np.ndarray:
    """One still of `recipe` on `rgb`: `photo_fx.core.runner.render` with the default subject finder."""
    return runner.render(rgb, recipe, matte, at, find_subject=_finder(find_subject))


def render_frames(
    rgb: np.ndarray, recipe: Recipe, matte: np.ndarray | None = None, *, find_subject: SubjectFinder | None = None
) -> Iterator[np.ndarray]:
    """Every frame of an animated recipe: `photo_fx.core.runner.render_frames` with the default subject finder."""
    return runner.render_frames(rgb, recipe, matte, find_subject=_finder(find_subject))


def trace(
    rgb: np.ndarray,
    recipe: Recipe,
    matte: np.ndarray | None = None,
    at: float = 0.0,
    *,
    find_subject: SubjectFinder | None = None,
) -> list[Stage]:
    """Every stage of a render: `photo_fx.core.runner.trace` with the default subject finder."""
    return runner.trace(rgb, recipe, matte, at, find_subject=_finder(find_subject))


def render_all(
    rgb: np.ndarray, recipes: list[Recipe], *, find_subject: SubjectFinder | None = None
) -> list[np.ndarray]:
    """Several recipes on one photo: `photo_fx.core.runner.render_all` with the default subject finder."""
    return runner.render_all(rgb, recipes, find_subject=_finder(find_subject))


def _finder(find_subject: SubjectFinder | None) -> SubjectFinder:
    # None stands for the default finder, so the signature matches the core's and the core never names `matte`.
    return subject.subject_matte if find_subject is None else find_subject


# Each function above is the core's, with the default finder bound: its one-line docstring is replaced by the core's
# full documentation, plus that.
for _public in (render, render_frames, trace, render_all):
    _public.__doc__ = (
        getattr(runner, _public.__name__).__doc__.rstrip()
        + "\n\n    Without `matte` or `find_subject`, the subject is found with `photo_fx.matte.subject_matte`.\n    "
    )
del _public

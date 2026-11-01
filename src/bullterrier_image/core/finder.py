"""Finding a map in a photo: `Finder`, a function with the name of its answers, `check_map`, the rule a map of a whole
photo is held to where it enters, and `find_map`, the one place a finder of a whole photo is called.

Dependency direction: imports `errors` and `ops.maps` only.
"""

from collections.abc import Callable
from dataclasses import dataclass

import numpy as np

from bullterrier_image.errors import DepthNotFoundError, MapNotFoundError, RecipeError, SubjectNotFoundError
from bullterrier_image.ops.maps import map_problem

__all__ = ["Finder", "answered", "check_map", "find_map"]


@dataclass(frozen=True)
class Finder:
    """A function that finds a map in a photo, and the name of its answers: `identity` changes exactly when they could.

    `find` takes a photo, float32 (h, w, 3) in 0..1, and returns its map, float32 (h, w) in 0..1 (for the subject: its
    alpha, white = subject; for depth: 1 = nearest), or raises the map's error (`SubjectNotFoundError`,
    `DepthNotFoundError`, else `MapNotFoundError`) when there is nothing to find. `identity` is non-blank text naming
    the function and whatever shapes its answers (model, version, options): two finders with one identity are taken to
    answer alike, so a cache keyed by it (`pipeline.run`) reuses their answers. RecipeError, naming the field, for a
    blank `identity` or a `find` that cannot be called.
    """

    identity: str
    find: Callable[[np.ndarray], np.ndarray]

    def __post_init__(self) -> None:
        if not isinstance(self.identity, str) or not self.identity.strip():
            raise RecipeError(f"Finder identity must be non-blank text naming its answers, got {self.identity!r}")
        if not callable(self.find):
            raise RecipeError(f"Finder find must be a function photo -> map, got {self.find!r}")


# What each map is, by name, for the messages about it; any other map is "the <name> map".
_WHAT = {"subject": "subject's alpha", "depth": "depth map (1 = nearest)"}


def _error(name: str) -> type[MapNotFoundError]:
    """The error for a problem with the map `name`: `SubjectNotFoundError` for the subject, `DepthNotFoundError` for the
    depth, `MapNotFoundError` for any other map."""
    return {"subject": SubjectNotFoundError, "depth": DepthNotFoundError}.get(name, MapNotFoundError)


def check_map(name: str, layer: np.ndarray, rgb: np.ndarray) -> np.ndarray:
    """`layer`, the map `name` of the photo `rgb`, once it is known to be the photo's (h, w), finite and within 0..1:
    the one check of a map of the whole photo, for a map given and a finder's answer alike, where it enters. The map's
    error (`SubjectNotFoundError` for the subject, `DepthNotFoundError` for the depth, else `MapNotFoundError`) names
    both sizes, or the least and greatest value."""
    error = _error(name)
    layer = np.asarray(layer)  # what a caller or a finder gives may be a list, or no map at all
    if layer.shape != rgb.shape[:2]:
        raise error(f"{name} is {layer.shape} but the photo is {rgb.shape[:2]}")
    if (problem := map_problem(layer)) is not None:
        raise error(f"{name} has {problem}: a map must be finite and within 0..1")
    return layer


def answered(name: str, answer: np.ndarray | None, rgb: np.ndarray) -> np.ndarray:
    """`answer`, what a finder gave for the map `name` of the photo `rgb`, checked (`check_map`). The map's error
    if it is None: a finder must answer or raise, and carrying on would drop the map without a word."""
    if answer is None:
        raise _error(name)(
            f"the finder of {name!r} returned None: it must return the {_WHAT.get(name, f'{name} map')} (float32, the "
            "photo's height and width, 0..1) or raise when it finds none"
        )
    return check_map(name, answer, rgb)


def find_map(name: str, finder: Finder, rgb: np.ndarray) -> np.ndarray:
    """Ask `finder` for the map `name` ("subject", "depth" or a given map's name) of the photo `rgb` and return it,
    checked (`answered`)."""
    return answered(name, finder.find(rgb), rgb)

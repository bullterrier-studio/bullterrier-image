"""The effect library.

An effect is a function `(rgb, **params) -> rgb`: it takes the image representation described in `imageio`
and returns a new one of the same shape. Effects know nothing about files, mattes or recipes; where and how
strongly one applies is decided by the recipe that calls it. Add one by writing a function with the
`@effect` decorator in any module here and importing that module below.
"""

import numpy as np

from . import ascii, color, glitch, texture  # noqa: F401  (importing registers their effects)
from ._registry import all_effects, get


def apply(name: str, rgb: np.ndarray, **params) -> np.ndarray:
    """Run the effect called `name` with `params`; unknown names and parameters raise with the valid choices."""
    return get(name)(rgb, **params)


def describe() -> dict[str, str]:
    """Effect name -> its documentation, for `portrait-fx effects`."""
    return {name: (fn.__doc__ or "").strip() for name, fn in sorted(all_effects().items())}

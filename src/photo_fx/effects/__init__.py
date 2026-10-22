"""The effect library.

An effect is a function `(rgb, **params) -> rgb`: it takes the image representation described in `imageio`
and returns a new one of the same shape. Effects know nothing about files, mattes or recipes; where and how
strongly one applies is decided by the recipe that calls it. Add one by writing a function with the
`@effect(category=...)` decorator in any module here and importing that module below.
"""

import re

import numpy as np

from . import ascii, color, disperse, evaporate, glitch, texture  # noqa: F401  (importing registers their effects)
from ._registry import CATEGORIES, all_effects, categories, category, get

__all__ = ["CATEGORIES", "all_effects", "apply", "categories", "category", "describe", "get", "summary"]

# The end of a sentence: a full stop, ! or ? followed by a space or the end. Decimals ("0.5") and ranges ("0..1") are
# never followed by a space; "e.g." and "i.e." are, so they are skipped.
_SENTENCE_END = re.compile(r"(?<!e\.g)(?<!i\.e)[.!?](?=\s|$)")


def apply(name: str, rgb: np.ndarray, **params) -> np.ndarray:
    """Run the effect called `name` with `params`; unknown names and parameters raise with the valid choices."""
    return get(name)(rgb, **params)


def describe() -> dict[str, str]:
    """Effect name -> its documentation, for `photo-fx effects`."""
    return {name: (fn.__doc__ or "").strip() for name, fn in sorted(all_effects().items())}


def summary(name: str) -> str:
    """The first sentence of the documentation of the effect called `name`, on one line."""
    text = " ".join((get(name).__doc__ or "").split())
    end = _SENTENCE_END.search(text)
    return text[: end.end()] if end else text

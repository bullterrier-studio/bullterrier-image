"""The effect library.

An effect is a function `(rgb, **params) -> rgb`: it takes the image representation described in `imageio`
and returns a new one of the same shape. Effects know nothing about files, mattes or recipes; where and how
strongly one applies is decided by the recipe that calls it.

Add one by writing a function with the `@effect(category=...)` decorator in a new or existing module in this package.
Nothing else needs editing: importing the package imports every module in it whose name does not start with `_`, in
sorted order, and importing a module registers its effects. Modules starting with `_` hold shared helpers.
"""

import importlib
import pkgutil
import re

import numpy as np


def _import_every_module() -> None:
    for module in sorted(info.name for info in pkgutil.iter_modules(__path__)):
        if not module.startswith("_"):
            importlib.import_module(f"{__name__}.{module}")


# Before the names below are bound, so that a module named like one of them cannot replace it on the package.
_import_every_module()

from ._registry import CATEGORIES, all_effects, categories, category, get  # noqa: E402  (must follow the imports)

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

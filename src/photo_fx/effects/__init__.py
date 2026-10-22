"""The effect library.

An effect is a function `(rgb, **params) -> rgb`: it takes the image representation described in `imageio`
and returns a new one of the same shape. Effects know nothing about files, mattes or recipes; where and how
strongly one applies is decided by the recipe that calls it.

Add one by writing a function with the `@effect(category=...)` decorator (from `photo_fx.core.registry`) in a new
module in this package. Nothing else needs editing: importing the package imports every module in it whose name does
not start with `_`, in sorted order, and importing a module registers its effects. Effects never import each other:
shared code lives in `photo_fx.ops`.

The query names below (`get`, `apply`, `describe`...) are the registry's, re-exported so that `photo_fx.effects.get`
and friends keep working.
"""

import importlib
import pkgutil


def _import_every_module() -> None:
    for module in sorted(info.name for info in pkgutil.iter_modules(__path__)):
        if not module.startswith("_"):
            importlib.import_module(f"{__name__}.{module}")


# Before the names below are bound, so that a module named like one of them cannot replace it on the package.
_import_every_module()

# Re-exported for compatibility: these lived in this package before the registry moved to `core` (M1).
from photo_fx.core.registry import (  # noqa: E402  (must follow the imports)
    CATEGORIES,
    all_effects,
    apply,
    categories,
    category,
    describe,
    get,
    summary,
)

__all__ = ["CATEGORIES", "all_effects", "apply", "categories", "category", "describe", "get", "summary"]

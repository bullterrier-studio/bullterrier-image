"""The table of effects, kept apart so effect modules can register themselves without importing the package."""

from collections.abc import Callable

import numpy as np

Effect = Callable[..., np.ndarray]

_EFFECTS: dict[str, Effect] = {}


def effect(fn: Effect) -> Effect:
    """Decorator: make `fn(rgb, **params) -> rgb` available to recipes under its own name."""
    _EFFECTS[fn.__name__] = fn
    return fn


def get(name: str) -> Effect:
    try:
        return _EFFECTS[name]
    except KeyError:
        raise KeyError(f"unknown effect {name!r}; available: {', '.join(sorted(_EFFECTS))}") from None


def all_effects() -> dict[str, Effect]:
    return dict(_EFFECTS)

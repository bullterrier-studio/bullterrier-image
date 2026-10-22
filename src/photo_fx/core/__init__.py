"""The kernel: the effect registry and the contract types every effect is written against.

`core` imports nothing from photo_fx outside itself (import-linter enforces it), so it stays small and stable while
effects, adapters and the app change around it.
"""

from .registry import (
    CATEGORIES,
    Effect,
    all_effects,
    apply,
    categories,
    category,
    describe,
    effect,
    get,
    summary,
    unregister,
)
from .types import Color
from .vocabulary import DISCOURAGED

__all__ = [
    "CATEGORIES", "DISCOURAGED", "Color", "Effect", "all_effects", "apply", "categories", "category", "describe",
    "effect", "get", "summary", "unregister",
]  # fmt: skip

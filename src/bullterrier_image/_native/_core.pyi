# The stub of the Rust extension `bullterrier_image._native._core` (crates/python). The functions' documentation is the
# extension's own (`help(_core.apply)`); `uv run poe stubtest` (in `poe check`) checks this stub against it.

from collections.abc import Callable
from typing import Any

import numpy as np

__all__ = [
    "CoreError",
    "apply",
    "check",
    "effect",
    "effects",
    "params",
    "read_recipe",
    "register",
    "unregister",
    "vocabulary",
]

class CoreError(ValueError):
    kind: str

def apply(
    effect: str,
    rgb: np.ndarray,
    params: dict[str, Any],
    *,
    size_ratio: float = 1.0,
    partition: np.ndarray | None = None,
    fields: dict[str, np.ndarray] | None = None,
) -> np.ndarray: ...
def check(effect: str, params: dict[str, Any]) -> dict[str, Any]: ...
def effects() -> list[str]: ...
def params(effect: str) -> dict[str, Any]: ...
def effect(effect: str) -> dict[str, Any]: ...
def read_recipe(text: str, syntax: str) -> dict[str, Any]: ...
def vocabulary() -> dict[str, list[str]]: ...
def register(
    name: str,
    category: str,
    opacity: float,
    function: Callable[..., np.ndarray],
    *,
    inputs: list[str] = ...,
    fields: dict[str, str] | None = None,
) -> None: ...
def unregister(name: str) -> None: ...

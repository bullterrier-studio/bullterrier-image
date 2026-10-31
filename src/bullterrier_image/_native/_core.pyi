# The stub of the Rust extension `bullterrier_image._native._core` (crates/python). The functions' documentation is the
# extension's own (`help(_core.apply)`); `uv run poe stubtest` (in `poe check`) checks this stub against it.

from collections.abc import Callable, Mapping
from typing import Any, Protocol

import numpy as np

__all__ = [
    "CoreError",
    "apply",
    "check",
    "check_recipe",
    "effect",
    "effects",
    "params",
    "read_recipe",
    "register",
    "render",
    "trace",
    "unregister",
    "vocabulary",
]

class CoreError(ValueError):
    kind: str

class _Finder(Protocol):
    def find(self, rgb: np.ndarray) -> np.ndarray | None: ...

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
def check_recipe(recipe: dict[str, Any], *, presets: Mapping[str, str] | None = None) -> None: ...
def effects(*, declared: bool = False) -> list[str]: ...
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
def render(
    rgb: np.ndarray,
    recipe: dict[str, Any],
    *,
    maps: Mapping[str, np.ndarray] | None = None,
    finders: Mapping[str, _Finder] | None = None,
    presets: Mapping[str, str] | None = None,
    size: int | None = None,
) -> np.ndarray: ...
def trace(
    rgb: np.ndarray,
    recipe: dict[str, Any],
    *,
    maps: Mapping[str, np.ndarray] | None = None,
    finders: Mapping[str, _Finder] | None = None,
    presets: Mapping[str, str] | None = None,
    size: int | None = None,
) -> list[tuple[str, int | None, np.ndarray]]: ...

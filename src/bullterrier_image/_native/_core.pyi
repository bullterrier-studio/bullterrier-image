# The stub of the Rust extension `bullterrier_image._native._core` (crates/python). The functions' documentation is the
# extension's own (`help(_core.apply)`); `uv run poe stubtest` (in `poe check`) checks this stub against it.

import os
from collections.abc import Callable, Mapping, Sequence
from typing import Any, Protocol, final

import numpy as np

__all__ = [
    "CoreError",
    "Lut",
    "apply",
    "check",
    "check_recipe",
    "effect",
    "effects",
    "known_maps",
    "needs",
    "params",
    "read",
    "read_map",
    "read_recipe",
    "region_presets",
    "register",
    "render",
    "trace",
    "unregister",
    "vocabulary",
    "write",
]

class CoreError(ValueError):
    kind: str

class _Finder(Protocol):
    def find(self, rgb: np.ndarray) -> np.ndarray | None: ...

@final
class Lut:
    def __new__(
        cls,
        table: np.ndarray,
        domain_min: Sequence[float] = ...,
        domain_max: Sequence[float] = ...,
    ) -> Lut: ...
    @staticmethod
    def identity(size: int) -> Lut: ...
    @staticmethod
    def from_cube(path: str | os.PathLike[str]) -> Lut: ...
    @property
    def size(self) -> int: ...
    @property
    def domain_min(self) -> tuple[float, float, float]: ...
    @property
    def domain_max(self) -> tuple[float, float, float]: ...
    def table(self) -> np.ndarray: ...

def apply(
    effect: str,
    rgb: np.ndarray,
    params: dict[str, Any],
    *,
    size_ratio: float = 1.0,
    partition: np.ndarray | None = None,
    fields: dict[str, np.ndarray] | None = None,
    luts: Mapping[str, Lut] | None = None,
    pictures: Mapping[str, np.ndarray] | None = None,
) -> np.ndarray: ...
def check(effect: str, params: dict[str, Any]) -> dict[str, Any]: ...
def check_recipe(recipe: dict[str, Any], *, presets: Mapping[str, str] | None = None) -> None: ...
def effects(*, declared: bool = False) -> list[str]: ...
def known_maps(recipe: dict[str, Any]) -> list[str]: ...
def needs(recipe: dict[str, Any], *, presets: Mapping[str, str] | None = None) -> dict[str, Any]: ...
def params(effect: str) -> dict[str, Any]: ...
def effect(effect: str) -> dict[str, Any]: ...
def read_recipe(text: str, syntax: str) -> dict[str, Any]: ...
def region_presets(kind: str) -> dict[str, dict[str, Any]]: ...
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
    luts: Mapping[str, Lut] | None = None,
    pictures: Mapping[str, np.ndarray] | None = None,
    presets: Mapping[str, str] | None = None,
    size: int | None = None,
    threads: int = 1,
) -> np.ndarray: ...
def trace(
    rgb: np.ndarray,
    recipe: dict[str, Any],
    *,
    maps: Mapping[str, np.ndarray] | None = None,
    finders: Mapping[str, _Finder] | None = None,
    luts: Mapping[str, Lut] | None = None,
    pictures: Mapping[str, np.ndarray] | None = None,
    presets: Mapping[str, str] | None = None,
    size: int | None = None,
    threads: int = 1,
) -> list[tuple[str, int | str | None, np.ndarray]]: ...
def read(path: str | os.PathLike[str]) -> np.ndarray: ...
def read_map(path: str | os.PathLike[str]) -> np.ndarray: ...
def write(path: str | os.PathLike[str], image: np.ndarray, quality: int = 95) -> None: ...

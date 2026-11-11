# The stub of the Rust extension `bullterrier_image._native._core` (crates/python). The functions' documentation is the
# extension's own (`help(_core.apply)`); `uv run poe stubtest` (in `poe check`) checks this stub against it.

import os
import pathlib
from collections.abc import Callable, Mapping, Sequence
from typing import Any, Protocol, final

import numpy as np

__all__ = [
    "CoreError",
    "Lut",
    "apply",
    "check",
    "check_project",
    "check_recipe",
    "contact_sheet",
    "depth_finder_identity",
    "documentation",
    "effect",
    "effects",
    "find_depth",
    "find_inverse_depth",
    "find_subject",
    "init_project",
    "kept_onnx_runtime",
    "known_maps",
    "locate",
    "main",
    "needs",
    "number_pieces",
    "numbers",
    "params",
    "parse_color",
    "project_maps",
    "read",
    "read_map",
    "read_project",
    "read_recipe",
    "read_recipe_source",
    "refine_edge",
    "region_presets",
    "register",
    "render",
    "resize",
    "run_project",
    "shipped_looks",
    "shipped_presets",
    "subject_finder_identity",
    "summary",
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
def numbers(recipe: dict[str, Any], foreign: dict[str, Any] | None = None) -> list[dict[str, Any]]: ...
def locate(recipe: dict[str, Any], address: str, foreign: dict[str, Any] | None = None) -> dict[str, Any]: ...
def needs(recipe: dict[str, Any], *, presets: Mapping[str, str] | None = None) -> dict[str, Any]: ...
def params(effect: str) -> dict[str, Any]: ...
def effect(effect: str) -> dict[str, Any]: ...
def read_recipe(text: str, syntax: str) -> dict[str, Any]: ...
def find_subject(
    rgb: np.ndarray,
    *,
    segmenter: str | None = None,
    shrink: float = 1.5,
    feather: float = 1.0,
    library: str | os.PathLike[str] | None = None,
    model_file: str | os.PathLike[str] | None = None,
    cache: str | os.PathLike[str] | None = None,
    allow_download: bool = False,
) -> np.ndarray: ...
def find_depth(
    rgb: np.ndarray,
    *,
    library: str | os.PathLike[str] | None = None,
    model_file: str | os.PathLike[str] | None = None,
    cache: str | os.PathLike[str] | None = None,
    allow_download: bool = False,
) -> np.ndarray: ...
def find_inverse_depth(
    rgb: np.ndarray,
    *,
    library: str | os.PathLike[str] | None = None,
    model_file: str | os.PathLike[str] | None = None,
    cache: str | os.PathLike[str] | None = None,
    allow_download: bool = False,
) -> np.ndarray: ...
def subject_finder_identity(
    *,
    segmenter: str | None = None,
    library: str | os.PathLike[str] | None = None,
    model_file: str | os.PathLike[str] | None = None,
) -> str: ...
def depth_finder_identity(
    *, library: str | os.PathLike[str] | None = None, model_file: str | os.PathLike[str] | None = None
) -> str: ...
def kept_onnx_runtime(*, cache: str | os.PathLike[str] | None = None) -> pathlib.Path: ...
def init_project(directory: str | os.PathLike[str]) -> pathlib.Path: ...
def read_project(path: str | os.PathLike[str]) -> dict[str, Any]: ...
def check_project(given: Mapping[str, Any]) -> dict[str, Any]: ...
def project_maps(given: Mapping[str, Any]) -> list[str]: ...
def run_project(
    given: Mapping[str, Any],
    finders: Mapping[str, tuple[str, Any]],
    *,
    pictures: Mapping[str, np.ndarray] | None = None,
    luts: Mapping[str, Lut] | None = None,
    force: bool = False,
    dry_run: bool = False,
    only: str | None = None,
    generated: str = "",
    log: Callable[[str], None] | None = None,
) -> list[dict[str, Any]]: ...
def contact_sheet(tiles: Sequence[tuple[str, np.ndarray]], columns: int, tile: int) -> np.ndarray: ...
def refine_edge(matte: np.ndarray, shrink: float, feather: float) -> np.ndarray: ...
def region_presets(kind: str) -> dict[str, dict[str, Any]]: ...
def shipped_presets() -> dict[str, str]: ...
def main(argv: list[str], version: str) -> int: ...
def read_recipe_source(spec: str, file: bool) -> dict[str, Any]: ...
def documentation(effect: str, *, advanced: bool = True) -> str: ...
def summary(effect: str) -> str: ...
def shipped_looks() -> list[tuple[str, str]]: ...
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
    threads: int = ...,
    band: int | None = None,
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
    threads: int = ...,
) -> list[tuple[str, int | str | None, np.ndarray]]: ...
def number_pieces(rgb: np.ndarray, partition: np.ndarray) -> np.ndarray: ...
def parse_color(value: object) -> tuple[float, float, float] | None: ...
def resize(image: np.ndarray, longest_side: int) -> np.ndarray: ...
def read(path: str | os.PathLike[str]) -> np.ndarray: ...
def read_map(path: str | os.PathLike[str]) -> np.ndarray: ...
def write(path: str | os.PathLike[str], image: np.ndarray, quality: int = 95) -> None: ...

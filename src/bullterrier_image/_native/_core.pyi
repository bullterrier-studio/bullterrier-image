# The stub of the Rust extension `bullterrier_image._native._core` (crates/python). The functions' documentation is the
# extension's own (`help(_core.apply)`); `uv run poe stubtest` (in `poe check`) checks this stub against it.

from typing import Any

import numpy as np

__all__ = ["apply", "effects", "params"]

def apply(effect: str, rgb: np.ndarray, params: dict[str, Any]) -> np.ndarray: ...
def effects() -> list[str]: ...
def params(effect: str) -> dict[str, Any]: ...

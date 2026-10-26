"""Running an ONNX model file on the CPU through ONNX Runtime, as every portable model of `finding` does.

The session of each file is loaded once and kept, so a second photo does not pay the second it takes to load.
"""

from functools import lru_cache
from pathlib import Path
from typing import Any

import numpy as np

from bullterrier_image.errors import BackendUnavailableError


def first_output(model: Path, batch: np.ndarray) -> Any:  # an array of the model's own shape and type
    """The first output of the ONNX model file `model` given `batch` as its first input.

    Raises ImportError when onnxruntime is not installed, and BackendUnavailableError naming `model` when onnxruntime
    cannot use it: not an ONNX model, or one whose input does not take `batch` (another size, rank or type).
    """
    from onnxruntime.capi import onnxruntime_pybind11_state as state  # onnxruntime's own errors, all since 1.17

    try:
        session = _session(_identity(model))
        first = session.get_outputs()[0].name
        (answer,) = session.run([first], {session.get_inputs()[0].name: batch})
    except (state.Fail, state.InvalidArgument, state.InvalidGraph, state.InvalidProtobuf, state.NoSuchFile) as error:
        raise BackendUnavailableError(f"onnxruntime cannot use the model file {model} ({error})") from error
    return answer


def _identity(model: Path) -> tuple[str, int, int]:
    """The model file as a cache key: a file replaced on disk is loaded again."""
    stat = model.stat()
    return str(model.resolve()), stat.st_size, stat.st_mtime_ns


@lru_cache(maxsize=2)
def _session(identity: tuple[str, int, int]) -> Any:  # an onnxruntime.InferenceSession; loading takes a second
    """The model's session, kept for the next call; `identity` (path, size, modification time) is the cache key."""
    import onnxruntime

    path, _, _ = identity
    return onnxruntime.InferenceSession(path, providers=["CPUExecutionProvider"])

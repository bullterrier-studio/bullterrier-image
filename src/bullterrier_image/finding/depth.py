"""Finding the depth of a picture: how near each pixel is.

`depth_map(rgb)` answers it at the picture's own size, 1 = nearest, 0 = farthest; `depth_finder()` is the same as a
`Finder` that the renderers take (`find_depth=`), named by its model. The model is Depth Anything V2 Small (Apache-2.0)
on ONNX Runtime, the same on macOS, Linux and Windows. It tells relative depth, not distance: the answer is scaled per
photo, so the nearest pixel of every photo is 1 and the farthest 0.

The model (about 99 MB) is not part of the package: the first use downloads it into the user's cache folder
(`depth_model_path()`), checks its SHA-256 and keeps it there; nothing is downloaded at import time. Offline, download
the file by hand (the error says from where and to where) or set `BT_IMAGE_DEPTH_MODEL` to a local copy, which is then
used as it is and never downloaded.

No model at all is needed when the caller already has a depth map: every renderer takes `depth=` (load a greyscale
picture, white = nearest, with `imageio.load_depth`, or pass `--depth` on the command line).
"""

import hashlib
import os
from pathlib import Path

import numpy as np

from bullterrier_image.core.runner import Finder
from bullterrier_image.errors import BackendUnavailableError
from bullterrier_image.ops.resample import stretch

from . import _depth_anything
from .download import DownloadError, fetch, user_cache_dir

MODEL_VARIABLE = "BT_IMAGE_DEPTH_MODEL"
_TRIED = "cannot find the depth with the depth model (Depth Anything V2 Small)"


def depth_map(rgb: np.ndarray) -> np.ndarray:
    """Depth map (height, width, float32 0..1) of `rgb`, same size as `rgb`: 1 = nearest, 0 = farthest.

    The model's answer, stretched to the photo's size, is scaled so its nearest pixel is 1 and its farthest 0; when it
    tells nothing apart (every pixel equally far) the map is all 0.

    Raises BackendUnavailableError when the model cannot run here: `BT_IMAGE_DEPTH_MODEL` names no file, the download
    fails (the message says how to get the file by hand), or onnxruntime is not installed.
    """
    model = _model()
    try:
        answer = _depth_anything.inverse_depth(rgb, model)
    except ImportError as error:
        raise BackendUnavailableError(f"{_TRIED}: it needs onnxruntime (pip install onnxruntime): {error}") from error
    nearness = stretch(answer, *rgb.shape[:2])
    low = nearness.min()
    span = nearness.max() - low  # in float32 as the values are, so the nearest pixel becomes exactly 1
    return (nearness - low) / span if span > 0 else np.zeros_like(nearness)


def depth_model_path() -> Path:
    """The model file the depth finder uses: `BT_IMAGE_DEPTH_MODEL` if set, else its place in the user's cache.

    The file need not exist yet: without `BT_IMAGE_DEPTH_MODEL`, the first `depth_map` downloads it there.
    """
    return _named() or user_cache_dir() / _depth_anything.MODEL_FILE


def depth_finder() -> Finder:
    """`depth_map` as a `Finder`. Its identity is a hash of the model: the pinned one when the standard model is meant
    (it is not downloaded to find out), else of the file `BT_IMAGE_DEPTH_MODEL` names, so the same model at another
    path is the same finder. Raises BackendUnavailableError when `BT_IMAGE_DEPTH_MODEL` names no file."""
    local = _local_model()
    if local is None:
        digest = _depth_anything.MODEL_SHA256
    else:
        with local.open("rb") as file:
            digest = hashlib.file_digest(file, "sha256").hexdigest()
    return Finder(f"onnx:sha256 {digest}", depth_map)


def _named() -> Path | None:
    """The file `BT_IMAGE_DEPTH_MODEL` names, or None when it is unset: the one place the variable is read."""
    named = os.environ.get(MODEL_VARIABLE)
    return Path(named) if named else None


def _local_model() -> Path | None:
    """The model file `BT_IMAGE_DEPTH_MODEL` names, which must exist; None when the standard model is meant."""
    named = _named()
    if named is not None and not named.is_file():
        raise BackendUnavailableError(f"{_TRIED}: {MODEL_VARIABLE} names {named}, which is not a file")
    return named


def _model() -> Path:
    """The model file to run: the local one, else the standard one, downloaded into the cache on first use."""
    local = _local_model()
    if local is not None:
        return local
    model = depth_model_path()
    try:
        return fetch(_depth_anything.MODEL_URL, _depth_anything.MODEL_SHA256, model, size=_depth_anything.MODEL_SIZE)
    except DownloadError as error:
        raise BackendUnavailableError(
            f"{_TRIED}: {error}. Or set {MODEL_VARIABLE} to a local copy, or give the depth map yourself (--depth)"
        ) from error

"""Finding the subject of a picture: the person, pet or object that stands out.

One question, one answer: `subject_matte(rgb)` returns how much each pixel belongs to the subject (1) or to the
background (0), at the picture's own size. Two backends can answer it:

- `"vision"`: Apple Vision's subject segmentation (the "lift subject" feature). macOS only; runs on the Neural Engine,
  needs no download; on macOS before 14 it uses Vision's older people-only model.
- `"onnx"`: a portable model, IS-Net (general use, Apache-2.0), run by ONNX Runtime on macOS, Linux and Windows. The
  model (about 180 MB) is not part of the package: the first use downloads it into the user's cache folder
  (`onnx_model_path()`), checks its SHA-256 and keeps it there; nothing is downloaded at import time. Offline, download
  the file by hand (the error says from where and to where) or set `PHOTO_FX_MODEL` to a local copy, which is then used
  as it is and never downloaded.

The backend is chosen by, in order: the `segmenter` argument; the `PHOTO_FX_SEGMENTER` environment variable (`vision` or
`onnx`); otherwise Apple Vision where it is available, and the portable model everywhere else.

No model at all is needed when the caller already has the subject's matte: every renderer takes `matte=` (load a
black-and-white or transparent PNG with `imageio.load_matte`, or pass `--matte` on the command line).
"""

import os
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage

from . import _onnx, _vision
from .download import DownloadError, fetch, user_cache_dir

SEGMENTERS = ("vision", "onnx")
SEGMENTER_VARIABLE = "PHOTO_FX_SEGMENTER"
MODEL_VARIABLE = "PHOTO_FX_MODEL"
_HALF = 0.5  # a pixel is the subject's when the model is at least this sure: the even split of a probability
_EIGHT_BIT = 255.0  # the largest value of an 8-bit mask


def subject_matte(
    rgb: np.ndarray, *, shrink: float = 1.5, feather: float = 1.0, segmenter: str | None = None
) -> np.ndarray:
    """Alpha matte (height, width, float32 0..1) for the main subject of `rgb`, same size as `rgb`.

    The segmenter's edge is soft and slightly generous, which shows as a coloured halo once the background is
    replaced. `shrink` pulls the edge in by that many pixels and `feather` blurs it by that many pixels,
    both measured in `rgb`'s own pixels; the defaults give a clean, slightly soft cut for a 1000-1600 px photo.
    `segmenter` is `"vision"` or `"onnx"`; None leaves the choice to `PHOTO_FX_SEGMENTER`, then to what this computer
    has (see the module documentation).

    Raises ValueError for an unknown segmenter name, RuntimeError if nothing stands out or the chosen backend cannot run
    here (the message says what was tried and what to do instead).
    """
    name, why = _choose(segmenter)
    mask = _vision_mask(rgb, why) if name == "vision" else _onnx_mask(rgb, why)
    eight_bit = mask is not None and mask.dtype == np.uint8  # Vision answers in 0..255, the portable model in 0..1
    if mask is None or not (mask >= (_HALF * _EIGHT_BIT if eight_bit else _HALF)).any():
        raise RuntimeError("no subject found in the picture (no person, pet or object stood out)")
    height, width = rgb.shape[:2]
    alpha = np.asarray(Image.fromarray(mask).resize((width, height), Image.Resampling.BILINEAR), np.float32)
    if eight_bit:
        alpha /= _EIGHT_BIT
    return refine_edge(alpha, shrink, feather)


def onnx_model_path() -> Path:
    """The model file the portable backend uses: `PHOTO_FX_MODEL` if set, else its place in the user's cache folder.

    The file need not exist yet: without `PHOTO_FX_MODEL`, the first `subject_matte` that needs it downloads it there.
    """
    local = os.environ.get(MODEL_VARIABLE)
    return Path(local) if local else user_cache_dir() / _onnx.MODEL_FILE


def _choose(segmenter: str | None) -> tuple[str, str]:
    """(backend name, how it was chosen, for error messages)."""
    if segmenter is not None:
        if segmenter not in SEGMENTERS:
            raise ValueError(f"unknown segmenter {segmenter!r}: choose one of {', '.join(SEGMENTERS)}")
        return segmenter, f"segmenter={segmenter!r}"
    named = os.environ.get(SEGMENTER_VARIABLE, "").strip().lower()
    if named:
        if named not in SEGMENTERS:
            choices = ", ".join(SEGMENTERS)
            raise ValueError(f"{SEGMENTER_VARIABLE}={named!r} is not a segmenter: set one of {choices}, or unset it")
        return named, f"{SEGMENTER_VARIABLE}={named}"
    reason = _vision.unavailable_reason()
    return ("vision", "the automatic choice") if reason is None else ("onnx", f"the automatic choice ({reason})")


def _vision_mask(rgb: np.ndarray, why: str) -> np.ndarray | None:
    reason = _vision.unavailable_reason()
    if reason is not None:
        raise RuntimeError(
            f"cannot find the subject with Apple Vision, chosen by {why}: {reason}. Choose the portable model "
            f"instead (segmenter 'onnx', or {SEGMENTER_VARIABLE}=onnx), or give the subject's matte yourself (--matte)"
        )
    return _vision.subject_mask(rgb)


def _onnx_mask(rgb: np.ndarray, why: str) -> np.ndarray:
    tried = f"cannot find the subject with the portable model (onnx, chosen by {why})"
    model = onnx_model_path()
    if os.environ.get(MODEL_VARIABLE):
        if not model.is_file():
            raise RuntimeError(f"{tried}: {MODEL_VARIABLE} names {model}, which is not a file")
    else:
        try:
            fetch(_onnx.MODEL_URL, _onnx.MODEL_SHA256, model)
        except DownloadError as error:
            raise RuntimeError(
                f"{tried}: {error}. Or set {MODEL_VARIABLE} to a local copy, or give the subject's matte yourself "
                "(--matte)"
            ) from error
    try:
        return _onnx.subject_probability(rgb, model)
    except ImportError as error:
        raise RuntimeError(f"{tried}: it needs onnxruntime (pip install onnxruntime): {error}") from error


def refine_edge(alpha: np.ndarray, shrink: float, feather: float) -> np.ndarray:
    """Clean up the edge of a matte (height, width, 0..1): returns a new float32 matte of the same size, in 0..1.

    `shrink` (px) pulls the edge in: a pixel keeps the lowest value within a disc of that radius around it, so a halo
    up to `shrink` px wide is cut away; the picture's own border does not count as an edge. Then `feather` (px) blurs
    the edge with a Gaussian of that sigma, which softens it without changing the matte's total. 0 turns either off,
    and so does a negative value (it is not an error).
    `subject_matte` applies this to the model's answer; use it on a matte from anywhere else for the same clean cut.
    """
    if shrink > 0:
        r = int(np.ceil(shrink))
        y, x = np.ogrid[-r : r + 1, -r : r + 1]
        alpha = ndimage.grey_erosion(alpha, footprint=(x * x + y * y) <= shrink * shrink + 1e-9, mode="nearest")
    if feather > 0:
        alpha = ndimage.gaussian_filter(alpha, feather)
    return np.clip(alpha, 0.0, 1.0).astype(np.float32)

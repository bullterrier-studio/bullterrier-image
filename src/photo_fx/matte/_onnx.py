"""The portable backend: IS-Net (general use) through ONNX Runtime, on macOS, Linux and Windows alike.

IS-Net is the salient-object model of "Highly Accurate Dichotomous Image Segmentation" (Qin et al., ECCV 2022,
https://github.com/xuebinqin/DIS, Apache-2.0). It finds whatever stands out (people, pets, objects), like Apple Vision.
The ONNX export is the one the rembg project publishes (https://github.com/danielgatis/rembg, MIT); its bytes are
pinned below by SHA-256, so a changed file is refused, not used.

The model contract (also what a local model file named by `PHOTO_FX_MODEL` must follow): one float32 input
(1, 3, 1024, 1024), the photo resized to 1024 x 1024, RGB, each value v / 255 - 0.5; the first output
(1, 1, 1024, 1024) is the probability (0..1) that a pixel belongs to the subject.
"""

from functools import lru_cache
from pathlib import Path
from typing import Any

import numpy as np
from PIL import Image

from photo_fx import imageio

MODEL_URL = "https://github.com/danielgatis/rembg/releases/download/v0.0.0/isnet-general-use.onnx"
MODEL_SHA256 = "60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a"
MODEL_FILE = "isnet-general-use.onnx"  # 178,648,008 bytes

# The model's published input (DIS repository, Inference.py): 1024 x 1024, value / 255 - 0.5 (mean 0.5, std 1).
_SIDE = 1024
_MEAN = 0.5
_EIGHT_BIT = 255.0  # the model was trained on 8-bit pictures
# ONNX Runtime's log level for warnings and above is 2; 3 keeps only errors, so a user's run is not filled with notices
# about graph optimisations that do not affect the result
_LOG_ERRORS_ONLY = 3


def subject_probability(rgb: np.ndarray, model: Path) -> np.ndarray:
    """float32 (1024, 1024) probability, 0..1, that each pixel of `rgb` (resized to the model's square) is the subject.

    model: the ONNX model file, which must follow the model contract in this module's docstring.
    Raises ImportError when onnxruntime is not installed.
    """
    session = _session(*_identity(model))
    square = imageio.to_pil(rgb).resize((_SIDE, _SIDE), Image.Resampling.BILINEAR)
    pixels = np.asarray(square, np.float32) / _EIGHT_BIT - _MEAN
    batch = pixels.transpose(2, 0, 1)[np.newaxis]  # (height, width, channel) -> (1, channel, height, width)
    first_output = session.get_outputs()[0].name
    (answer,) = session.run([first_output], {session.get_inputs()[0].name: batch})
    return np.clip(np.asarray(answer, np.float32).reshape(_SIDE, _SIDE), 0.0, 1.0)


def _identity(model: Path) -> tuple[str, int, int]:
    """The model file as a cache key: a file replaced on disk is loaded again."""
    stat = model.stat()
    return str(model.resolve()), stat.st_size, stat.st_mtime_ns


@lru_cache(maxsize=2)
def _session(path: str, size: int, modified: int) -> Any:  # an onnxruntime.InferenceSession; loading takes a second
    import onnxruntime

    options = onnxruntime.SessionOptions()
    options.log_severity_level = _LOG_ERRORS_ONLY
    return onnxruntime.InferenceSession(path, options, providers=["CPUExecutionProvider"])

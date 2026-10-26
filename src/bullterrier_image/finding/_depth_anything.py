"""The depth model: Depth Anything V2 Small through ONNX Runtime, on macOS, Linux and Windows alike.

Depth Anything V2 is the monocular depth model of "Depth Anything V2" (Yang et al., NeurIPS 2024,
https://github.com/DepthAnything/Depth-Anything-V2; the Small model is Apache-2.0). The ONNX export is the fp32 one
the onnx-community publishes (https://huggingface.co/onnx-community/depth-anything-v2-small), pinned to a commit; its
bytes are pinned below by SHA-256, so a changed file is refused, not used.

The model contract (also what a local model file named by `BT_IMAGE_DEPTH_MODEL` must follow): one float32 input
(1, 3, height, width), any height and width, the photo resized so its short side is 518 px and both sides are
multiples of 14, RGB, each value (v / 255 - mean) / std with ImageNet's mean and std per channel; the first output,
(1, height, width) or (1, 1, height, width), is relative inverse depth: larger is nearer, in no fixed unit or range.
"""

from pathlib import Path

import numpy as np
from PIL import Image

from bullterrier_image import imageio

from . import _onnx

_COMMIT = "f7421df0cc30f121782ab050d42f0a423291dcde"
MODEL_URL = f"https://huggingface.co/onnx-community/depth-anything-v2-small/resolve/{_COMMIT}/onnx/model.onnx"
MODEL_SHA256 = "afb6a5c28f3b6bf1618c6e43f02073ef9dfdc70e937502d51603e57b0a1df10c"
MODEL_FILE = "depth-anything-v2-small.onnx"
MODEL_SIZE = 99_060_839  # bytes

# The model's published input (Depth-Anything-V2, depth_anything_v2/dpt.py `image2tensor`; the export's
# preprocessor_config.json says the same): bicubic, short side 518, sides multiples of 14, ImageNet normalisation.
_SHORT_SIDE = 518  # constant: the input size the model was trained at (37 patches of 14 px)
_PATCH = 14  # constant: the ViT-S/14 encoder's patch size; each side must be a whole number of patches
_MEAN = np.array([0.485, 0.456, 0.406], np.float32)  # constant: ImageNet's per-channel mean (torchvision)
_STD = np.array([0.229, 0.224, 0.225], np.float32)  # constant: ImageNet's per-channel std (torchvision)
_EIGHT_BIT = 255.0  # constant: the model was trained on 8-bit pictures


def inverse_depth(rgb: np.ndarray, model: Path) -> np.ndarray:
    """float32 (height, width) relative inverse depth of `rgb` (larger = nearer) at the model's input size for it.

    model: the ONNX model file, which must follow the model contract in this module's docstring.
    Raises ImportError when onnxruntime is not installed.
    """
    height, width = _input_size(*rgb.shape[:2])
    resized = imageio.to_pil(rgb).resize((width, height), Image.Resampling.BICUBIC)
    pixels = (np.asarray(resized, np.float32) / _EIGHT_BIT - _MEAN) / _STD
    batch = pixels.transpose(2, 0, 1)[np.newaxis]  # (height, width, channel) -> (1, channel, height, width)
    return np.asarray(_onnx.first_output(model, batch), np.float32).reshape(height, width)


def _input_size(height: int, width: int) -> tuple[int, int]:
    """The model's input size for a photo of `height` x `width`: scaled so the short side is 518 px, then each side
    to the nearest multiple of 14 (the short side stays 518, itself a multiple)."""
    scale = _SHORT_SIDE / min(height, width)
    return round(height * scale / _PATCH) * _PATCH, round(width * scale / _PATCH) * _PATCH

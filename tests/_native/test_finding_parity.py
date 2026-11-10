"""The core's finders against the Python finders, which are the oracle until they are deleted: the same photo through
the same models (the library of the `onnxruntime` package) gives the same map to within the model's
sensitivity (Rust resizes the model input in float, Python rounds it to 8 bit first: 1/255 of input).

Real models, so only where they are in the cache (nothing is downloaded here) and, for Apple Vision, only on macOS."""

import sys
from collections.abc import Callable
from pathlib import Path

import numpy as np
import pytest

from bullterrier_image import imageio
from bullterrier_image._native import _core  # noqa: PLC2701  architecture: the Rust core is compared with Python's finders
from bullterrier_image.finding.depth import depth_map, depth_model_path
from bullterrier_image.finding.subject import onnx_model_path, subject_matte

PHOTO = imageio.load(Path(__file__).parents[2] / "examples" / "photos" / "bullterrier.jpeg")


def onnxruntime_library() -> Path:
    import onnxruntime

    capi = Path(onnxruntime.__file__).parent / "capi"
    return (
        next(capi.glob("*onnxruntime.*dylib"), None)
        or next(capi.glob("libonnxruntime.so*"), None)
        or capi / "onnxruntime.dll"
    )


def cached(monkeypatch: pytest.MonkeyPatch, variable: str, path: Callable[[], Path]) -> Path:
    """The model in the cache folder (the conftest points `variable` at a file that is not there, so no test downloads),
    or skip when it was never downloaded."""
    monkeypatch.delenv(variable)
    found = path()
    if not found.is_file():
        pytest.skip(f"the model is not in the cache ({found}); `bt-image cutout` downloads it")
    return found


def test_the_portable_subject_model_cuts_the_dog_as_python_does(monkeypatch):
    model = cached(monkeypatch, "BT_IMAGE_SUBJECT_MODEL", onnx_model_path)
    mine = _core.find_subject(PHOTO, segmenter="onnx", library=onnxruntime_library(), model_file=model)
    theirs = subject_matte(PHOTO, segmenter="onnx")
    assert mine.shape == theirs.shape
    assert ((mine >= 0.5) != (theirs >= 0.5)).mean() < 1e-3  # under 0.1 % of the pixels change side
    assert np.abs(mine - theirs).max() < 0.05


def test_the_depth_model_gives_the_dog_the_depth_python_does(monkeypatch):
    model = cached(monkeypatch, "BT_IMAGE_DEPTH_MODEL", depth_model_path)
    mine = _core.find_depth(PHOTO, library=onnxruntime_library(), model_file=model)
    theirs = depth_map(PHOTO)
    assert mine.shape == theirs.shape
    assert np.abs(mine - theirs).max() < 0.05


@pytest.mark.skipif(sys.platform != "darwin", reason="Apple Vision is macOS only")
def test_apple_vision_cuts_the_dog_as_pyobjc_does():
    mine = _core.find_subject(PHOTO, segmenter="vision")
    theirs = subject_matte(PHOTO, segmenter="vision")
    assert mine.shape == theirs.shape
    np.testing.assert_allclose(mine, theirs, atol=1e-5)  # the same calls, the same pixels: the same mask


def test_a_segmenter_that_is_unknown_is_refused_naming_the_choices():
    with pytest.raises(ValueError, match="vision, onnx"):
        _core.find_subject(PHOTO, segmenter="coreml")

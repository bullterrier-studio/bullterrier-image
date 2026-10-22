"""The Apple Vision backend: macOS's own subject segmentation (the "lift subject" feature), through pyobjc.

Runs on the Neural Engine, needs no download, and works for people, pets and objects alike. On macOS before 14 only
the older people-only model exists, and it is used instead. pyobjc is imported inside the functions, so the rest of
the package works where it is not installed.
"""

import io
import sys
from typing import Any

import numpy as np

from photo_fx import imageio


def unavailable_reason() -> str | None:
    """Why Vision cannot run on this computer (a phrase for an error message), or None when it can."""
    if sys.platform != "darwin":
        return "Apple Vision is macOS only"
    try:
        import Quartz  # noqa: F401  (imported only to learn whether pyobjc is installed)
        import Vision  # noqa: F401  (imported only to learn whether pyobjc is installed)
    except ImportError as error:
        return f"Apple Vision needs pyobjc-framework-vision and pyobjc-framework-quartz ({error})"
    return None


def subject_mask(rgb: np.ndarray) -> np.ndarray | None:
    """uint8 mask (0..255) of every foreground subject at Vision's own resolution, or None when Vision found none."""
    mask = _foreground_mask(rgb)
    if mask is None:  # macOS before 14: only the people model exists
        mask = _person_mask(rgb)
    return mask


def _handler(rgb: np.ndarray) -> Any:  # a VNImageRequestHandler; pyobjc has no type stubs
    """A Vision request handler for `rgb`."""
    import Foundation
    import Vision

    png = io.BytesIO()
    imageio.to_pil(rgb).save(png, format="PNG")
    data = Foundation.NSData.dataWithBytes_length_(png.getvalue(), len(png.getvalue()))
    return Vision.VNImageRequestHandler.alloc().initWithData_options_(data, {})


def _read_mask(buffer: Any) -> np.ndarray:  # a CVPixelBuffer
    """uint8 mask (0..255) from a Vision pixel buffer holding one 8-bit or 32-bit float channel."""
    import Quartz

    Quartz.CVPixelBufferLockBaseAddress(buffer, 1)
    try:
        w, h = Quartz.CVPixelBufferGetWidth(buffer), Quartz.CVPixelBufferGetHeight(buffer)
        stride = Quartz.CVPixelBufferGetBytesPerRow(buffer)
        raw = Quartz.CVPixelBufferGetBaseAddress(buffer).as_buffer(stride * h)
        if Quartz.CVPixelBufferGetPixelFormatType(buffer) == Quartz.kCVPixelFormatType_OneComponent32Float:
            values = np.frombuffer(raw, dtype=np.float32).reshape(h, stride // 4)[:, :w]
            return (np.clip(values, 0.0, 1.0) * 255.0 + 0.5).astype(np.uint8)
        return np.frombuffer(raw, dtype=np.uint8).reshape(h, stride)[:, :w].copy()
    finally:
        Quartz.CVPixelBufferUnlockBaseAddress(buffer, 1)


def _foreground_mask(rgb: np.ndarray) -> np.ndarray | None:
    """uint8 mask of every foreground subject (people, pets, objects), or None if Vision found none or is too old."""
    import Vision

    if not hasattr(Vision, "VNGenerateForegroundInstanceMaskRequest"):  # macOS 14 and later
        return None
    handler = _handler(rgb)
    request = Vision.VNGenerateForegroundInstanceMaskRequest.alloc().init()
    ok, _ = handler.performRequests_error_([request], None)
    if not ok or not request.results():
        return None
    observation = request.results()[0]
    buffer, _ = observation.generateScaledMaskForImageForInstances_fromRequestHandler_error_(
        observation.allInstances(), handler, None
    )
    return None if buffer is None else _read_mask(buffer)


def _person_mask(rgb: np.ndarray) -> np.ndarray | None:
    """uint8 mask (0..255) of the people in the picture from Vision, or None. The older, people-only model."""
    import Quartz
    import Vision

    request = Vision.VNGeneratePersonSegmentationRequest.alloc().initWithCompletionHandler_(None)
    request.setQualityLevel_(Vision.VNGeneratePersonSegmentationRequestQualityLevelAccurate)
    request.setOutputPixelFormat_(Quartz.kCVPixelFormatType_OneComponent8)
    ok, _ = _handler(rgb).performRequests_error_([request], None)
    return _read_mask(request.results()[0].pixelBuffer()) if ok and request.results() else None

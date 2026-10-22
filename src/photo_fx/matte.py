"""Finding the subject of a picture: the person, pet or object that stands out.

One question, one answer: `subject_matte(rgb)` returns how much each pixel belongs to the subject (1) or to the
background (0). Which model produces that is this module's business; callers never see it.

Today the model is Apple Vision's foreground-subject segmentation (the "lift subject" feature): macOS only, runs on the
Neural Engine, no download, and works for people, pets and objects alike. On macOS before 14 it falls back to Vision's
older people-only model. Another backend (for Linux) would be selected inside `subject_matte`.
"""

import io
import sys

import numpy as np
from PIL import Image
from scipy import ndimage

from . import imageio


def subject_matte(rgb: np.ndarray, *, shrink: float = 1.5, feather: float = 1.0) -> np.ndarray:
    """Alpha matte (height, width, float32 0..1) for the main subject of `rgb`, same size as `rgb`.

    The segmenter's edge is soft and slightly generous, which shows as a coloured halo once the background is
    replaced. `shrink` pulls the edge in by that many pixels and `feather` blurs it by that many pixels,
    both measured in `rgb`'s own pixels; the defaults give a clean, slightly soft cut for a 1000-1600 px photo.

    Raises RuntimeError if nothing stands out, NotImplementedError off macOS.
    """
    if sys.platform != "darwin":
        raise NotImplementedError("subject_matte needs Apple Vision, which is macOS only")
    height, width = rgb.shape[:2]
    mask = _vision_foreground_mask(rgb)
    if mask is None:  # macOS before 14: only the people model exists
        mask = _vision_person_mask(rgb)
    if mask is None or not (mask >= 128).any():
        raise RuntimeError("no subject found in the picture (no person, pet or object stood out)")
    mask = np.asarray(Image.fromarray(mask).resize((width, height), Image.Resampling.BILINEAR))
    return _refine_edge(mask.astype(np.float32) / 255.0, shrink, feather)


def _refine_edge(alpha: np.ndarray, shrink: float, feather: float) -> np.ndarray:
    """Erode by `shrink` px (pulls the edge back inside the body), then blur by `feather` px (sigma)."""
    if shrink > 0:
        r = int(np.ceil(shrink))
        y, x = np.ogrid[-r : r + 1, -r : r + 1]
        alpha = ndimage.grey_erosion(alpha, footprint=(x * x + y * y) <= shrink * shrink + 1e-9, mode="nearest")
    if feather > 0:
        alpha = ndimage.gaussian_filter(alpha, feather)
    return np.clip(alpha, 0.0, 1.0).astype(np.float32)


def _vision_handler(rgb: np.ndarray):
    """A Vision request handler for `rgb` (imported here so the rest of the package works without pyobjc)."""
    import Foundation
    import Vision

    png = io.BytesIO()
    imageio.to_pil(rgb).save(png, format="PNG")
    data = Foundation.NSData.dataWithBytes_length_(png.getvalue(), len(png.getvalue()))
    return Vision.VNImageRequestHandler.alloc().initWithData_options_(data, {})


def _read_mask(buffer) -> np.ndarray:
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


def _vision_foreground_mask(rgb: np.ndarray) -> np.ndarray | None:
    """uint8 mask of every foreground subject (people, pets, objects), or None if Vision found none or is too old."""
    import Vision

    if not hasattr(Vision, "VNGenerateForegroundInstanceMaskRequest"):  # macOS 14 and later
        return None
    handler = _vision_handler(rgb)
    request = Vision.VNGenerateForegroundInstanceMaskRequest.alloc().init()
    ok, _ = handler.performRequests_error_([request], None)
    if not ok or not request.results():
        return None
    observation = request.results()[0]
    buffer, _ = observation.generateScaledMaskForImageForInstances_fromRequestHandler_error_(
        observation.allInstances(), handler, None
    )
    return None if buffer is None else _read_mask(buffer)


def _vision_person_mask(rgb: np.ndarray) -> np.ndarray | None:
    """uint8 mask (0..255) of the people in the picture from Vision, or None. The older, people-only model."""
    import Quartz
    import Vision

    request = Vision.VNGeneratePersonSegmentationRequest.alloc().initWithCompletionHandler_(None)
    request.setQualityLevel_(Vision.VNGeneratePersonSegmentationRequestQualityLevelAccurate)
    request.setOutputPixelFormat_(Quartz.kCVPixelFormatType_OneComponent8)
    ok, _ = _vision_handler(rgb).performRequests_error_([request], None)
    return _read_mask(request.results()[0].pixelBuffer()) if ok and request.results() else None

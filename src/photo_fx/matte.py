"""Finding the person in a picture.

One question, one answer: `subject_matte(rgb)` returns how much each pixel belongs to the person (1) or
to the background (0). Which model produces that is this module's business; callers never see it.

Today the model is Apple Vision's person segmentation: macOS only, runs on the Neural Engine, no download.
Another backend (for Linux, or for non-person subjects) would be selected inside `subject_matte`.
"""

import io
import sys

import numpy as np
from PIL import Image
from scipy import ndimage

from . import imageio


def subject_matte(rgb: np.ndarray, *, shrink: float = 1.5, feather: float = 1.0) -> np.ndarray:
    """Alpha matte (height, width, float32 0..1) for the main person in `rgb`, same size as `rgb`.

    The segmenter's edge is soft and slightly generous, which shows as a coloured halo once the background is
    replaced. `shrink` pulls the edge in by that many pixels and `feather` blurs it by that many pixels,
    both measured in `rgb`'s own pixels; the defaults give a clean, slightly soft cut for a 1000-1600 px portrait.

    Raises RuntimeError if no person is found, NotImplementedError off macOS.
    """
    if sys.platform != "darwin":
        raise NotImplementedError("subject_matte needs Apple Vision, which is macOS only")
    height, width = rgb.shape[:2]
    mask = _vision_person_mask(rgb)
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


def _vision_person_mask(rgb: np.ndarray) -> np.ndarray:
    """uint8 mask (0..255) from Vision, at Vision's own resolution, covering the whole picture."""
    # Imported here so the rest of the package (and its tests) work on machines without pyobjc.
    import Foundation
    import Quartz
    import Vision

    png = io.BytesIO()
    imageio.to_pil(rgb).save(png, format="PNG")
    data = Foundation.NSData.dataWithBytes_length_(png.getvalue(), len(png.getvalue()))

    request = Vision.VNGeneratePersonSegmentationRequest.alloc().initWithCompletionHandler_(None)
    request.setQualityLevel_(Vision.VNGeneratePersonSegmentationRequestQualityLevelAccurate)
    request.setOutputPixelFormat_(Quartz.kCVPixelFormatType_OneComponent8)
    handler = Vision.VNImageRequestHandler.alloc().initWithData_options_(data, {})
    ok, error = handler.performRequests_error_([request], None)
    if not ok or not request.results():
        raise RuntimeError(f"Vision person segmentation failed: {error}")

    buffer = request.results()[0].pixelBuffer()
    Quartz.CVPixelBufferLockBaseAddress(buffer, 1)
    try:
        w, h = Quartz.CVPixelBufferGetWidth(buffer), Quartz.CVPixelBufferGetHeight(buffer)
        stride = Quartz.CVPixelBufferGetBytesPerRow(buffer)
        raw = Quartz.CVPixelBufferGetBaseAddress(buffer).as_buffer(stride * h)
        mask = np.frombuffer(raw, dtype=np.uint8).reshape(h, stride)[:, :w].copy()
    finally:
        Quartz.CVPixelBufferUnlockBaseAddress(buffer, 1)
    if not (mask >= 128).any():
        raise RuntimeError("no person found in the picture")
    return mask

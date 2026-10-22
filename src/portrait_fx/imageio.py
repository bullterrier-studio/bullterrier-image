"""Reading, writing and resizing pictures.

Every other module works on a single image representation: a float32 array of shape (height, width, 3),
RGB, values 0..1, sRGB-encoded (display-referred, not linear). This module is the only place that knows
about file formats, EXIF rotation, ICC profiles and 8-bit conversion, so none of that leaks into effects.
"""

import io
from pathlib import Path

import numpy as np
from PIL import Image, ImageCms, ImageOps

_SRGB = ImageCms.createProfile("sRGB")


def load(path: str | Path) -> np.ndarray:
    """Read a photo as RGB float32 0..1, upright and in sRGB.

    Phone photos carry an EXIF rotation and often a Display P3 profile; both are applied here so that
    what you see in Preview is what the effects get.
    """
    with Image.open(path) as opened:
        img = ImageOps.exif_transpose(opened)
        profile = img.info.get("icc_profile")
        if profile:
            try:
                source = ImageCms.ImageCmsProfile(io.BytesIO(profile))
                img = ImageCms.profileToProfile(img.convert("RGB"), source, _SRGB, outputMode="RGB")
            except ImageCms.PyCMSError:
                pass  # unreadable profile: treat the pixels as sRGB, which is what most viewers do too
        return from_pil(img)


def save(rgb: np.ndarray, path: str | Path, alpha: np.ndarray | None = None, quality: int = 95) -> None:
    """Write `rgb` to `path`; the extension picks the format.

    With `alpha` (height, width, 0..1) the file gets a transparency channel, so use .png or .webp.
    """
    img = to_pil(rgb, alpha)
    if alpha is None and Path(path).suffix.lower() in {".jpg", ".jpeg"}:
        img.save(path, quality=quality, subsampling=0)
    else:
        img.save(path)


def to_pil(rgb: np.ndarray, alpha: np.ndarray | None = None) -> Image.Image:
    """Convert to a Pillow image (RGB, or RGBA when `alpha` is given)."""
    data = rgb if alpha is None else np.dstack([rgb, alpha])
    return Image.fromarray((np.clip(data, 0.0, 1.0) * 255.0 + 0.5).astype(np.uint8))


def from_pil(img: Image.Image) -> np.ndarray:
    """Convert a Pillow image to RGB float32 0..1 (transparency is dropped)."""
    return np.asarray(img.convert("RGB"), dtype=np.float32) / 255.0


def resize(array: np.ndarray, longest_side: int) -> np.ndarray:
    """Scale an RGB image, or a (height, width) alpha, so its longest side is `longest_side` px.

    Aspect ratio is kept. Works on float data without going through 8 bits, so a matte keeps its softness.
    """
    height, width = array.shape[:2]
    scale = longest_side / max(height, width)
    size = (max(1, round(width * scale)), max(1, round(height * scale)))
    if size == (width, height):
        return array
    resample = Image.Resampling.LANCZOS if scale < 1 else Image.Resampling.BICUBIC
    channels = [array] if array.ndim == 2 else [array[..., c] for c in range(array.shape[2])]
    scaled = [np.asarray(Image.fromarray(c, mode="F").resize(size, resample)) for c in channels]
    out = scaled[0] if array.ndim == 2 else np.dstack(scaled)
    return np.clip(out, 0.0, 1.0).astype(np.float32)
